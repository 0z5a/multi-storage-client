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

//! Run: `cargo run --example s3_profile -- CONFIG_FILE PROFILE`.
//! Uses an existing S3 bucket and a fresh UUID namespace for every invocation.
mod support;

use bytes::Bytes;
use futures::StreamExt;
use multi_storage_client::{
    ConfigLoader, CopyOptions, DeleteOptions, ListOptions, ObjectPath, StorageClient, WriteOptions,
};
use std::{ffi::OsString, path::PathBuf};
use support::ExampleResult;
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> ExampleResult<()> {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    let [path, profile] = arguments.as_slice() else {
        return Err("usage: s3_profile CONFIG_FILE PROFILE".into());
    };
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err("the explicit configuration file must exist".into());
    }
    let profile = profile.to_str().ok_or("profile name must be UTF-8")?;
    let config = ConfigLoader::new().with_explicit_paths([path]).load()?;
    let prefix = ObjectPath::new(format!("rust-example/{}", uuid::Uuid::new_v4()))?;
    let client = StorageClient::builder(config, profile).build().await?;
    let mut created = Vec::new();
    let result = demonstrate(&client, &prefix, &mut created).await;
    // Delete only keys whose publication succeeded in this invocation. An
    // ambiguous failed write is not assumed to be owned by this example.
    let cleanup = cleanup(&client, &created).await;
    support::finish(result.and(cleanup), &client).await
}

fn exclusive() -> WriteOptions {
    WriteOptions {
        if_none_match: Some("*".into()),
        ..WriteOptions::default()
    }
}

async fn demonstrate(
    client: &StorageClient,
    prefix: &ObjectPath,
    created: &mut Vec<ObjectPath>,
) -> ExampleResult<()> {
    let greeting = prefix.join(&ObjectPath::new("greeting.txt")?)?;
    let copy = prefix.join(&ObjectPath::new("copy.txt")?)?;
    let multipart = prefix.join(&ObjectPath::new("multipart.bin")?)?;
    client
        .write(
            &greeting,
            Bytes::from_static(b"hello from Rust"),
            exclusive(),
        )
        .await?;
    created.push(greeting.clone());
    if client.read(&greeting).await? != "hello from Rust" {
        return Err("S3 roundtrip returned unexpected greeting bytes".into());
    }
    client
        .copy(
            &greeting,
            &copy,
            CopyOptions {
                destination: exclusive(),
                ..CopyOptions::default()
            },
        )
        .await?;
    created.push(copy);

    // Writer memory stays bounded while its private disk spool grows. finish
    // With the default 8 MiB threshold, finish uploads this 10 MiB object using
    // multipart. A supplied profile may configure a different threshold.
    // Conditional multipart writes are unsupported. This fresh UUID namespace
    // lets the large object use unconditional write options safely.
    let mut writer = client
        .writer(&multipart, Some(10 * 1024 * 1024), WriteOptions::default())
        .await?;
    let chunk = vec![0x6a; 1024 * 1024];
    for _ in 0..10 {
        writer.write_all(&chunk).await?;
    }
    let uploaded = writer.finish().await?;
    created.push(multipart);
    if uploaded.content_length != 10 * 1024 * 1024 {
        return Err("S3 upload returned an unexpected object length".into());
    }

    let mut listing = client
        .list(ListOptions {
            prefix: prefix.clone(),
            ..ListOptions::default()
        })
        .await?;
    let mut count = 0;
    while let Some(row) = listing.next().await {
        let row = row?;
        println!("{} ({} bytes)", row.key, row.content_length);
        count += 1;
    }
    if count != 3 {
        return Err("S3 listing did not return the three published objects".into());
    }
    println!("S3 roundtrip and multipart publication completed; cleaning 3 owned keys");
    Ok(())
}

async fn cleanup(client: &StorageClient, created: &[ObjectPath]) -> ExampleResult<()> {
    let report = client
        .delete_many(created, DeleteOptions::default())
        .await?;
    if let Some(failure) = report.failed.into_iter().next() {
        return Err(failure.error.into());
    }
    Ok(())
}
