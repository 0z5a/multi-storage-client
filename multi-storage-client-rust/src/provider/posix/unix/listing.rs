// SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Lazy lexical traversal with bounded per-directory sorting and owned spools.

use super::{PosixProvider, blocking, errno};
use crate::{
    ErrorKind, ListOptions, MetadataStream, MscError, ObjectKind, ObjectMetadata, ObjectPath,
    Result,
    provider::sort::{MetadataSorter, Records},
};
use futures::stream;
use rustix::fs;
use std::collections::BTreeMap;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub(super) fn open(provider: PosixProvider, options: ListOptions) -> Result<MetadataStream> {
    options.validate()?;
    let state = Listing {
        provider,
        options,
        initialized: false,
        pending: BTreeMap::new(),
    };
    Ok(Box::pin(stream::try_unfold(
        state,
        |mut state| async move {
            Ok::<_, MscError>(state.next().await?.map(|metadata| (metadata, state)))
        },
    )))
}

struct DirectoryRecords {
    records: Records<ObjectMetadata>,
    _directory: tempfile::TempDir,
}

struct ListedObject {
    metadata: ObjectMetadata,
    source: Option<DirectoryRecords>,
}

enum Pending {
    Object(Box<ListedObject>),
    Directory(ObjectPath),
}

struct Listing {
    provider: PosixProvider,
    options: ListOptions,
    initialized: bool,
    pending: BTreeMap<String, Pending>,
}

impl Listing {
    async fn initialize(&mut self) -> Result<()> {
        self.initialized = true;
        let provider = self.provider.clone();
        let key = self.options.prefix.clone();
        let metadata = match blocking(move || provider.metadata_with_attributes(&key, false)).await
        {
            Ok(metadata) => metadata,
            Err(error) if matches!(error.kind, ErrorKind::NotFound | ErrorKind::Unsupported) => {
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        if metadata.kind == ObjectKind::Directory {
            self.schedule_directory(metadata.key);
        } else {
            self.pending.insert(
                metadata.key.to_string(),
                Pending::Object(Box::new(ListedObject {
                    metadata,
                    source: None,
                })),
            );
        }
        Ok(())
    }

    fn schedule_directory(&mut self, key: ObjectPath) {
        // Expand at `key/`, so sibling `key.txt` precedes descendants `key/file`.
        let order = if key.is_root() {
            String::new()
        } else {
            format!("{key}/")
        };
        if self
            .options
            .end_at
            .as_ref()
            .is_none_or(|end| order.as_str() <= end.as_str())
        {
            self.pending.insert(order, Pending::Directory(key));
        }
    }

    async fn insert_source(&mut self, mut source: DirectoryRecords) -> Result<()> {
        if let Some(metadata) = source.records.next().await? {
            self.pending.insert(
                metadata.key.to_string(),
                Pending::Object(Box::new(ListedObject {
                    metadata,
                    source: Some(source),
                })),
            );
        }
        Ok(())
    }

    async fn next(&mut self) -> Result<Option<ObjectMetadata>> {
        if !self.initialized {
            self.initialize().await?;
        }
        while let Some((_, item)) = self.pending.pop_first() {
            match item {
                Pending::Directory(key) => {
                    let source = read_directory(&self.provider, key).await?;
                    self.insert_source(source).await?;
                }
                Pending::Object(item) => {
                    let ListedObject { metadata, source } = *item;
                    if let Some(source) = source {
                        self.insert_source(source).await?;
                    }
                    if metadata.kind == ObjectKind::Directory {
                        if self.options.recursive {
                            self.schedule_directory(metadata.key.clone());
                        }
                        if !self.options.include_directories {
                            continue;
                        }
                    }
                    if self.options.contains(&metadata.key) {
                        if self.options.include_attributes && metadata.kind == ObjectKind::File {
                            let provider = self.provider.clone();
                            return blocking(move || provider.metadata(&metadata.key))
                                .await
                                .map(Some);
                        }
                        return Ok(Some(metadata));
                    }
                }
            }
        }
        Ok(None)
    }
}

async fn read_directory(provider: &PosixProvider, key: ObjectPath) -> Result<DirectoryRecords> {
    let directory = tempfile::Builder::new()
        .prefix(".msc-tmp-list-")
        .tempdir()?;
    let mut sorter = MetadataSorter::new(directory.path().to_path_buf(), CancellationToken::new());
    let permit = provider
        .semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| MscError::new(ErrorKind::Cancelled, "POSIX provider closed"))?;
    let provider = provider.clone();
    let (sender, mut receiver) = mpsc::channel(32);
    let (finished, completion) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("msc-posix-list".into())
        .spawn(move || {
            let result = (|| {
                let _permit = permit;
                let descriptor = provider.directory(&key, false)?;
                let reader = fs::Dir::read_from(&descriptor).map_err(errno)?;
                for entry in reader {
                    if sender.is_closed() {
                        return Ok(());
                    }
                    let entry = entry.map_err(errno)?;
                    let name = entry.file_name().to_str().map_err(|_| {
                        MscError::new(ErrorKind::InvalidPath, "POSIX listing name is not UTF-8")
                    })?;
                    if matches!(name, "." | "..") || name.starts_with(".msc-tmp-") {
                        continue;
                    }
                    let child = key.join(&ObjectPath::new(name)?)?;
                    let metadata = match provider.metadata_with_attributes(&child, false) {
                        Ok(metadata) => metadata,
                        Err(error) if error.kind == ErrorKind::Unsupported => continue,
                        Err(error) => return Err(error),
                    };
                    if sender.blocking_send(metadata).is_err() {
                        return Ok(());
                    }
                }
                Ok::<_, MscError>(())
            })();
            let _ = finished.send(result);
        })?;
    while let Some(metadata) = receiver.recv().await {
        sorter.push(metadata).await?;
    }
    completion
        .await
        .map_err(|_| MscError::new(ErrorKind::Cancelled, "POSIX listing task failed"))??;
    let path = sorter.finish().await?;
    Ok(DirectoryRecords {
        records: Records::open(&path).await?,
        _directory: directory,
    })
}
