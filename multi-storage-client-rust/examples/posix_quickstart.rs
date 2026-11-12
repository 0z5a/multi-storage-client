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

//! Run: `cargo run --example posix_quickstart`.
//! All files, including upload/download fixtures, live in temporary directories.
mod support;

use bytes::Bytes;
use futures::StreamExt;
use multi_storage_client::{
    ByteRange, ConfigLoader, CopyOptions, DeleteOptions, InfoOptions, ListOptions, ObjectPath,
    ReadOptions, StorageClient, WriteOptions,
};
use std::{io::SeekFrom, path::Path};
use support::ExampleResult;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> ExampleResult<()> {
    let storage = tempfile::tempdir()?;
    let local_files = tempfile::tempdir()?;
    let config_path = local_files.path().join("config.json");
    std::fs::write(
        &config_path,
        serde_json::to_vec(
            &serde_json::json!({"profiles": {"local": {"storage_provider": {"type": "file", "options": {"base_path": storage.path()}}}}}),
        )?,
    )?;
    let config = ConfigLoader::new()
        .with_explicit_paths([config_path])
        .load()?;
    let client = StorageClient::builder(config, "local").build().await?;
    let result = demonstrate(&client, local_files.path()).await;
    support::finish(result, &client).await
}

async fn demonstrate(client: &StorageClient, local: &Path) -> ExampleResult<()> {
    // Bytes APIs suit small objects. Larger objects can use readers, writers,
    // upload_from, and download_to without buffering the entire object in RAM.
    let written = client
        .write(
            "texts/greeting.txt",
            Bytes::from_static(b"hello, storage!"),
            WriteOptions::default(),
        )
        .await?;
    assert_eq!(written.content_length, 15);
    assert_eq!(client.read("texts/greeting.txt").await?, "hello, storage!");
    let metadata = client
        .info("texts/greeting.txt", InfoOptions::default())
        .await?;
    println!(
        "created {} ({} bytes)",
        metadata.key, metadata.content_length
    );

    // ByteRange takes an offset and a length, rather than an inclusive end.
    let range = client
        .read_range(
            "texts/greeting.txt",
            ByteRange::new(7, 7)?,
            ReadOptions::default(),
        )
        .await?;
    assert_eq!(range, "storage");
    let mut reader = client
        .reader("texts/greeting.txt", ReadOptions::default())
        .await?;
    reader.seek(SeekFrom::Start(7)).await?;
    let mut word = [0_u8; 7];
    reader.read_exact(&mut word).await?;
    assert_eq!(&word, b"storage");
    println!("range and seek returned storage");

    // flush drains writes; shutdown closes writer input; only finish publishes.
    let mut writer = client
        .writer("logs/streamed.txt", None, WriteOptions::default())
        .await?;
    writer.write_all(b"first chunk\n").await?;
    writer.write_all(b"second chunk\n").await?;
    writer.shutdown().await?;
    assert!(!client.is_file("logs/streamed.txt").await?);
    let published = writer.finish().await?;
    println!("finished writer ({} bytes)", published.content_length);

    client
        .copy(
            "texts/greeting.txt",
            "texts/backup.txt",
            CopyOptions::default(),
        )
        .await?;
    client
        .write(
            "texts/greeting.txt",
            Bytes::from_static(b"updated greeting"),
            WriteOptions::default(),
        )
        .await?;
    assert_eq!(client.read("texts/greeting.txt").await?, "updated greeting");

    let input = local.join("input.txt");
    let downloaded = local.join("downloaded.txt");
    tokio::fs::write(&input, b"uploaded file").await?;
    client
        .upload_from("files/imported.txt", &input, WriteOptions::default())
        .await?;
    client
        .download_to("files/imported.txt", &downloaded, ReadOptions::default())
        .await?;
    assert_eq!(tokio::fs::read(&downloaded).await?, b"uploaded file");

    print_ordered_listing(client).await?;

    // Batch indices refer to the original input, even when completion order differs.
    let files = [
        (
            ObjectPath::new("files/imported.txt")?,
            local.join("batch-good.txt"),
        ),
        (
            ObjectPath::new("missing.txt")?,
            local.join("batch-missing.txt"),
        ),
    ];
    let batch = client.download_many(&files, ReadOptions::default()).await?;
    assert_eq!(batch.succeeded, [0]);
    assert_eq!(batch.failed.len(), 1);
    let failure = &batch.failed[0];
    assert_eq!(failure.index, 1);
    println!(
        "batch input {} ({}) failed: {:?}",
        failure.index, failure.source, failure.error.kind
    );

    client
        .delete("texts/backup.txt", false, DeleteOptions::default())
        .await?;
    assert!(!client.is_file("texts/backup.txt").await?);
    println!("deleted backup; temporary files are removed after shutdown");
    Ok(())
}

async fn print_ordered_listing(client: &StorageClient) -> ExampleResult<()> {
    // Poll ordered rows directly. Production consumers need not collect a tree.
    let mut listing = client.list(ListOptions::default()).await?;
    let mut previous = None;
    while let Some(row) = listing.next().await {
        let row = row?;
        assert!(previous.as_ref().is_none_or(|key| key < &row.key));
        println!("list {} ({} bytes)", row.key, row.content_length);
        previous = Some(row.key);
    }
    Ok(())
}
