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

//! Paged lexical listing with bounded grouping for synthesized directory keys.

use super::{S3Provider, transport::translate};
use crate::{
    ErrorKind, ListOptions, MetadataStream, MscError, ObjectKind, ObjectMetadata, ObjectPath,
    Result, StorageProvider,
    provider::sort::{MetadataSorter, Records},
};
use futures::stream;
use object_store::{
    Attributes,
    list::{PaginatedListOptions, PaginatedListStore},
};
use std::collections::{BTreeMap, VecDeque};
use tokio_util::sync::CancellationToken;

pub(super) fn list(provider: S3Provider, options: &ListOptions) -> Result<MetadataStream> {
    options.validate()?;
    let listing = Listing {
        provider,
        options: options.clone(),
        page: VecDeque::new(),
        token: None,
        done: false,
        last: None,
        group: None,
        output: None,
    };
    Ok(Box::pin(stream::try_unfold(
        listing,
        |mut listing| async move {
            Ok::<_, MscError>(listing.next().await?.map(|metadata| (metadata, listing)))
        },
    )))
}

struct Entry {
    expanded: bool,
    order: String,
    metadata: ObjectMetadata,
}

struct Listing {
    provider: S3Provider,
    options: ListOptions,
    page: VecDeque<Entry>,
    token: Option<String>,
    done: bool,
    last: Option<ObjectPath>,
    group: Option<Group>,
    output: Option<GroupOutput>,
}

impl Listing {
    async fn fetch(&mut self) -> Result<()> {
        let path = self.provider.path(&self.options.prefix)?;
        let prefix = if self.options.recursive && !self.options.prefix.is_root() {
            path.to_string()
        } else if path.as_ref().is_empty() {
            String::new()
        } else {
            format!("{path}/")
        };
        let previous = self.token.clone();
        let offset = self
            .options
            .start_after
            .as_ref()
            .map(|key| self.provider.path(key).map(|path| path.to_string()))
            .transpose()?;
        let page = self
            .provider
            .store
            .list_paginated(
                Some(&prefix),
                PaginatedListOptions {
                    delimiter: (!self.options.recursive).then(|| "/".into()),
                    max_keys: Some(self.provider.options.list_page_size),
                    page_token: previous.clone(),
                    offset,
                    ..Default::default()
                },
            )
            .await
            .map_err(|error| translate(error, "list", self.options.prefix.as_str()))?;
        if previous.is_some() && page.page_token == previous {
            return Err(MscError::new(
                ErrorKind::Integrity,
                "S3 continuation token did not advance",
            ));
        }
        self.token = page.page_token;
        self.done = self.token.is_none();
        let mut entries = Vec::new();
        for object in page.result.objects {
            let metadata = self.provider.metadata(object, &Attributes::new())?;
            entries.push(Entry {
                expanded: false,
                order: metadata.key.to_string(),
                metadata,
            });
        }
        if self.options.include_directories && !self.options.recursive {
            for prefix in page.result.common_prefixes {
                let key = self.provider.relative(&prefix)?;
                entries.push(Entry {
                    expanded: true,
                    order: format!("{key}/"),
                    metadata: ObjectMetadata::directory(key),
                });
            }
        }
        entries.sort_unstable_by(|left, right| left.order.cmp(&right.order));
        self.page = entries.into();
        Ok(())
    }

    async fn next_ordered(&mut self) -> Result<Option<ObjectMetadata>> {
        loop {
            if let Some(output) = &mut self.output {
                if let Some(metadata) = output.next().await? {
                    return Ok(Some(metadata));
                }
                self.output = None;
            }
            if self.page.front().is_some_and(|entry| {
                self.last
                    .as_ref()
                    .is_some_and(|last| entry.metadata.key <= *last)
            }) {
                self.page.pop_front();
                continue;
            }
            if let Some(entry) = self.page.front_mut()
                && self.options.recursive
                && self.options.include_directories
                && !entry.expanded
            {
                entry.expanded = true;
                let mut ancestors = Vec::new();
                for (index, character) in entry.metadata.key.as_str().char_indices() {
                    if character != '/' {
                        continue;
                    }
                    let key = ObjectPath::new(&entry.metadata.key.as_str()[..index])?;
                    if key.as_str().starts_with(self.options.prefix.as_str())
                        && key != self.options.prefix
                        && self.last.as_ref().is_none_or(|last| key > *last)
                    {
                        ancestors.push(Entry {
                            expanded: true,
                            order: format!("{key}/"),
                            metadata: ObjectMetadata::directory(key),
                        });
                    }
                }
                for ancestor in ancestors.into_iter().rev() {
                    self.page.push_front(ancestor);
                }
            }
            if let Some(entry) = self.page.front() {
                if self
                    .group
                    .as_ref()
                    .is_some_and(|group| entry.order > group.end)
                {
                    self.output = Some(self.group.take().unwrap().finish().await?);
                    continue;
                }
                let entry = self.page.pop_front().unwrap();
                let group_end = self
                    .options
                    .include_directories
                    .then(|| {
                        group_end(
                            &entry.metadata.key,
                            &self.options.prefix,
                            self.last.as_ref(),
                        )
                    })
                    .flatten();
                if self.group.is_none() {
                    let earliest = group_end
                        .as_ref()
                        .map_or(entry.metadata.key.as_str(), |end| &end[..end.len() - 1]);
                    if self
                        .options
                        .end_at
                        .as_ref()
                        .is_some_and(|end| earliest > end.as_str())
                    {
                        return Ok(None);
                    }
                    if let Some(end) = group_end {
                        self.group = Some(Group::new(end));
                    }
                }
                if let Some(group) = &mut self.group {
                    let reaches_end = entry.order == group.end;
                    group.push(entry.metadata).await?;
                    if reaches_end {
                        self.output = Some(self.group.take().unwrap().finish().await?);
                    }
                } else {
                    return Ok(Some(entry.metadata));
                }
            } else if self.done {
                if let Some(group) = self.group.take() {
                    self.output = Some(group.finish().await?);
                } else {
                    return Ok(None);
                }
            } else {
                self.fetch().await?;
            }
        }
    }

    async fn next(&mut self) -> Result<Option<ObjectMetadata>> {
        while let Some(mut metadata) = self.next_ordered().await? {
            if self
                .options
                .end_at
                .as_ref()
                .is_some_and(|end| metadata.key > *end)
            {
                return Ok(None);
            }
            if !self.options.contains(&metadata.key)
                || self.last.as_ref().is_some_and(|last| metadata.key <= *last)
            {
                continue;
            }
            if metadata.kind == ObjectKind::File
                && (self.options.include_attributes || metadata.content_length == 0)
            {
                metadata = self.provider.head(&metadata.key).await?;
            }
            if !self.options.include_attributes {
                metadata.attributes.clear();
            }
            self.last = Some(metadata.key.clone());
            return Ok(Some(metadata));
        }
        Ok(None)
    }
}

// S3 orders `a-000` before `a/`, but our directory key is `a`. Hold only the
// affected key range until its possible directory prefix has been encountered.
fn group_end(key: &ObjectPath, prefix: &ObjectPath, last: Option<&ObjectPath>) -> Option<String> {
    for (index, ch) in key.as_str().char_indices() {
        if index == 0 || ch >= '/' {
            continue;
        }
        let Ok(candidate) = ObjectPath::new(&key.as_str()[..index]) else {
            continue;
        };
        if candidate.as_str().starts_with(prefix.as_str())
            && candidate != *prefix
            && last.is_none_or(|last| candidate > *last)
        {
            return Some(format!("{candidate}/"));
        }
    }
    None
}

struct Spool {
    sorter: MetadataSorter,
    directory: tempfile::TempDir,
}

struct Group {
    end: String,
    entries: BTreeMap<ObjectPath, ObjectMetadata>,
    bytes: usize,
    spool: Option<Spool>,
}

impl Group {
    fn new(end: String) -> Self {
        Self {
            end,
            entries: BTreeMap::new(),
            bytes: 0,
            spool: None,
        }
    }

    async fn push(&mut self, metadata: ObjectMetadata) -> Result<()> {
        if let Some(spool) = &mut self.spool {
            return spool.sorter.push(metadata).await;
        }
        if let Some(previous) = self.entries.get(&metadata.key) {
            if metadata.kind == ObjectKind::Directory {
                return Ok(());
            }
            self.bytes -= metadata_size(previous)?;
        }
        self.bytes += metadata_size(&metadata)?;
        self.entries.insert(metadata.key.clone(), metadata);
        if self.entries.len() >= 1024 || self.bytes >= 8 * 1024 * 1024 {
            let directory = tempfile::Builder::new().prefix("msc-s3-list-").tempdir()?;
            let mut sorter = MetadataSorter::new_deduplicating(
                directory.path().to_path_buf(),
                CancellationToken::new(),
            );
            for metadata in std::mem::take(&mut self.entries).into_values() {
                sorter.push(metadata).await?;
            }
            self.spool = Some(Spool { sorter, directory });
            self.bytes = 0;
        }
        Ok(())
    }

    async fn finish(self) -> Result<GroupOutput> {
        if let Some(spool) = self.spool {
            let path = spool.sorter.finish().await?;
            Ok(GroupOutput::Disk {
                records: Records::open(&path).await?,
                _directory: spool.directory,
            })
        } else {
            Ok(GroupOutput::Memory(self.entries.into_values().collect()))
        }
    }
}

fn metadata_size(metadata: &ObjectMetadata) -> Result<usize> {
    serde_json::to_vec(metadata)
        .map(|bytes| bytes.len())
        .map_err(|_| MscError::new(ErrorKind::Integrity, "cannot encode S3 listing metadata"))
}

enum GroupOutput {
    Memory(VecDeque<ObjectMetadata>),
    Disk {
        records: Records<ObjectMetadata>,
        _directory: tempfile::TempDir,
    },
}

impl GroupOutput {
    async fn next(&mut self) -> Result<Option<ObjectMetadata>> {
        match self {
            Self::Memory(entries) => Ok(entries.pop_front()),
            Self::Disk { records, .. } => records.next().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn metadata_sorter_rejects_duplicates_within_chunks_and_across_runs() {
        for count in [10, 1100] {
            let directory = tempfile::tempdir().unwrap();
            let mut sorter =
                MetadataSorter::new(directory.path().to_path_buf(), CancellationToken::new());
            for index in 0..count {
                sorter
                    .push(ObjectMetadata::file(
                        ObjectPath::new(format!("a-{index:04}")).unwrap(),
                        42,
                    ))
                    .await
                    .unwrap();
            }
            sorter
                .push(ObjectMetadata::file(ObjectPath::new("a-0000").unwrap(), 42))
                .await
                .unwrap();
            assert_eq!(
                sorter.finish().await.unwrap_err().kind,
                ErrorKind::Integrity
            );
        }
    }

    #[tokio::test]
    async fn large_directory_group_spills_and_prefers_real_objects_over_prefixes() {
        let mut group = Group::new("a/".into());
        for index in (0..1100).rev() {
            group
                .push(ObjectMetadata::file(
                    ObjectPath::new(format!("a-{index:04}")).unwrap(),
                    42,
                ))
                .await
                .unwrap();
        }
        assert!(group.spool.is_some());
        group
            .push(ObjectMetadata::directory(
                ObjectPath::new("a-0000").unwrap(),
            ))
            .await
            .unwrap();
        group
            .push(ObjectMetadata::directory(
                ObjectPath::new("a-1099").unwrap(),
            ))
            .await
            .unwrap();
        group
            .push(ObjectMetadata::directory(ObjectPath::new("a").unwrap()))
            .await
            .unwrap();
        let mut output = group.finish().await.unwrap();
        let mut keys = Vec::new();
        while let Some(metadata) = output.next().await.unwrap() {
            if matches!(metadata.key.as_str(), "a-0000" | "a-1099") {
                assert_eq!(metadata.kind, ObjectKind::File);
                assert_eq!(metadata.content_length, 42);
            }
            keys.push(metadata.key);
        }
        assert_eq!(keys.len(), 1101);
        assert_eq!(keys[0].as_str(), "a");
        assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
