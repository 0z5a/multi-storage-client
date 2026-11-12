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

//! Private restrictive JSONL spools and a bounded external sort for provider listings.
use crate::{ErrorKind, MscError, ObjectKind, ObjectMetadata, Result};
use serde::{Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio_util::sync::CancellationToken;

const SORT_CHUNK_ENTRIES: usize = 1024;
const MAX_SORT_CHUNK_BYTES: usize = 8 * 1024 * 1024;
const MAX_RECORD_BYTES: u64 = 1024 * 1024;

pub(crate) async fn private_file(path: &Path) -> Result<tokio::fs::File> {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    Ok(options.open(path).await?)
}

pub(crate) async fn write_record<T: Serialize>(
    writer: &mut BufWriter<tokio::fs::File>,
    record: &T,
) -> Result<()> {
    let bytes = serde_json::to_vec(record)
        .map_err(|_| MscError::new(ErrorKind::Integrity, "cannot encode listing spool record"))?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(MscError::new(
            ErrorKind::Unsupported,
            "listing metadata exceeds the 1 MiB record limit",
        ));
    }
    writer.write_all(&bytes).await?;
    writer.write_all(b"\n").await?;
    Ok(())
}

pub(crate) struct Records<T> {
    reader: BufReader<tokio::fs::File>,
    marker: std::marker::PhantomData<T>,
}

impl<T: DeserializeOwned> Records<T> {
    pub(crate) async fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            reader: BufReader::new(tokio::fs::File::open(path).await?),
            marker: std::marker::PhantomData,
        })
    }

    pub(crate) async fn next(&mut self) -> Result<Option<T>> {
        let mut line = Vec::new();
        let size = (&mut self.reader)
            .take(MAX_RECORD_BYTES + 2)
            .read_until(b'\n', &mut line)
            .await?;
        if size == 0 {
            return Ok(None);
        }
        if size as u64 > MAX_RECORD_BYTES + 1 || line.last() != Some(&b'\n') {
            return Err(MscError::new(
                ErrorKind::Integrity,
                "oversized or truncated listing spool record",
            ));
        }
        serde_json::from_slice(&line)
            .map(Some)
            .map_err(|_| MscError::new(ErrorKind::Integrity, "invalid listing spool record"))
    }
}

/// At most 1024 records and 64 run paths are retained, independently of tree size.
pub(crate) struct MetadataSorter {
    directory: PathBuf,
    chunk: Vec<ObjectMetadata>,
    chunk_bytes: usize,
    levels: Vec<Option<PathBuf>>,
    next_run: u64,
    cancellation: CancellationToken,
    deduplicate: bool,
}

impl MetadataSorter {
    pub(crate) fn new(directory: PathBuf, cancellation: CancellationToken) -> Self {
        Self {
            directory,
            chunk: Vec::with_capacity(SORT_CHUNK_ENTRIES),
            chunk_bytes: 0,
            levels: Vec::new(),
            next_run: 0,
            cancellation,
            deduplicate: false,
        }
    }

    /// A cloud directory prefix can share a key with a real object. Prefer the
    /// object's metadata while keeping sync source duplicate checks strict.
    #[cfg(feature = "s3")]
    pub(crate) fn new_deduplicating(directory: PathBuf, cancellation: CancellationToken) -> Self {
        Self {
            deduplicate: true,
            ..Self::new(directory, cancellation)
        }
    }

    pub(crate) async fn push(&mut self, metadata: ObjectMetadata) -> Result<()> {
        let bytes = serde_json::to_vec(&metadata)
            .map_err(|_| MscError::new(ErrorKind::Integrity, "cannot encode source metadata"))?
            .len();
        if bytes as u64 > MAX_RECORD_BYTES {
            return Err(MscError::new(
                ErrorKind::Unsupported,
                "listing metadata exceeds the 1 MiB record limit",
            ));
        }
        if !self.chunk.is_empty() && self.chunk_bytes + bytes > MAX_SORT_CHUNK_BYTES {
            self.flush_chunk().await?;
        }
        self.chunk_bytes += bytes;
        self.chunk.push(metadata);
        if self.chunk.len() == SORT_CHUNK_ENTRIES {
            self.flush_chunk().await?;
        }
        Ok(())
    }

    fn run_path(&mut self) -> PathBuf {
        let path = self
            .directory
            .join(format!("source-run-{}.jsonl", self.next_run));
        self.next_run += 1;
        path
    }

    async fn flush_chunk(&mut self) -> Result<()> {
        if self.chunk.is_empty() {
            return Ok(());
        }
        self.chunk.sort_unstable_by(|left, right| {
            left.key.cmp(&right.key).then_with(|| {
                (left.kind == ObjectKind::Directory).cmp(&(right.kind == ObjectKind::Directory))
            })
        });
        if !self.deduplicate && self.chunk.windows(2).any(|pair| pair[0].key == pair[1].key) {
            return Err(MscError::new(ErrorKind::Integrity, "duplicate listing key"));
        }
        if self.deduplicate {
            self.chunk.dedup_by(|right, left| right.key == left.key);
        }
        let mut run = self.run_path();
        let mut writer = BufWriter::new(private_file(&run).await?);
        for metadata in self.chunk.drain(..) {
            write_record(&mut writer, &metadata).await?;
        }
        self.chunk_bytes = 0;
        writer.flush().await?;
        drop(writer);
        let mut level = 0;
        loop {
            if level == self.levels.len() {
                self.levels.push(None);
            }
            if let Some(previous) = self.levels[level].take() {
                let merged = self.run_path();
                merge_runs(
                    &previous,
                    &run,
                    &merged,
                    &self.cancellation,
                    self.deduplicate,
                )
                .await?;
                tokio::fs::remove_file(previous).await?;
                tokio::fs::remove_file(run).await?;
                run = merged;
                level += 1;
            } else {
                self.levels[level] = Some(run);
                return Ok(());
            }
        }
    }

    pub(crate) async fn finish(mut self) -> Result<PathBuf> {
        self.flush_chunk().await?;
        let runs: Vec<_> = self.levels.into_iter().flatten().collect();
        let mut current: Option<PathBuf> = None;
        for run in runs {
            if let Some(previous) = current.take() {
                let merged = self
                    .directory
                    .join(format!("source-final-{}.jsonl", self.next_run));
                self.next_run += 1;
                merge_runs(
                    &previous,
                    &run,
                    &merged,
                    &self.cancellation,
                    self.deduplicate,
                )
                .await?;
                tokio::fs::remove_file(previous).await?;
                tokio::fs::remove_file(run).await?;
                current = Some(merged);
            } else {
                current = Some(run);
            }
        }
        if let Some(path) = current {
            return Ok(path);
        }
        let path = self.directory.join("source-empty.jsonl");
        drop(private_file(&path).await?);
        Ok(path)
    }
}

async fn merge_runs(
    left: &Path,
    right: &Path,
    destination: &Path,
    cancellation: &CancellationToken,
    deduplicate: bool,
) -> Result<()> {
    let mut left = Records::<ObjectMetadata>::open(left).await?;
    let mut right = Records::<ObjectMetadata>::open(right).await?;
    let mut writer = BufWriter::new(private_file(destination).await?);
    let mut left_item = left.next().await?;
    let mut right_item = right.next().await?;
    while left_item.is_some() || right_item.is_some() {
        if cancellation.is_cancelled() {
            return Err(MscError::new(
                ErrorKind::Cancelled,
                "provider listing cancelled",
            ));
        }
        if let (Some(left_metadata), Some(right_metadata)) = (&left_item, &right_item)
            && left_metadata.key == right_metadata.key
            && deduplicate
        {
            let metadata = if left_metadata.kind == ObjectKind::Directory {
                right_metadata
            } else {
                left_metadata
            };
            write_record(&mut writer, metadata).await?;
            left_item = left.next().await?;
            right_item = right.next().await?;
            continue;
        }
        let take_left = match (&left_item, &right_item) {
            (Some(left), Some(right)) => {
                if left.key == right.key {
                    return Err(MscError::new(
                        ErrorKind::Integrity,
                        "source expansion generated duplicate keys",
                    ));
                }
                left.key < right.key
            }
            (Some(_), None) => true,
            _ => false,
        };
        if take_left {
            write_record(&mut writer, left_item.as_ref().expect("left item selected")).await?;
            left_item = left.next().await?;
        } else {
            write_record(
                &mut writer,
                right_item.as_ref().expect("right item selected"),
            )
            .await?;
            right_item = right.next().await?;
        }
    }
    writer.flush().await?;
    Ok(())
}
