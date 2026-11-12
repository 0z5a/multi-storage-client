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

//! Async-first multi-storage client for Rust.
//!
//! S3, S3-compatible and POSIX storage through a provider-neutral asynchronous API.
//! Callers own the Tokio runtime. Configuration, credentials, and factories are
//! separate from data operations; no process-global client is installed.
//!
//! # Getting started
//!
//! Start with [`ConfigLoader`] and [`StorageClient::builder`] to preserve profile
//! configuration. For a direct local adapter, use [`StorageClient::from_provider`].
//! All client data methods take keys relative to the profile's base path;
//! [`ResolvedConfig::resolve`] translates MSC URLs and absolute local paths.
//!
//! The following example is self-contained on Unix and writes only to a
//! temporary directory:
//!
//! ```
//! # #[cfg(unix)]
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use bytes::Bytes;
//! use multi_storage_client::{StorageClient, WriteOptions};
//! use multi_storage_client::provider::posix::{PosixOptions, PosixProvider};
//! use std::sync::Arc;
//!
//! let runtime = tokio::runtime::Runtime::new()?;
//! runtime.block_on(async {
//!     let directory = tempfile::tempdir()?;
//!     let provider = PosixProvider::new(PosixOptions::new(directory.path()))?;
//!     let client = StorageClient::from_provider(Arc::new(provider));
//!     let operation = async {
//!         client.write("hello.txt", Bytes::from_static(b"hello"), WriteOptions::default()).await?;
//!         assert_eq!(client.read("hello.txt").await?, "hello");
//!         Ok::<_, multi_storage_client::MscError>(())
//!     }.await;
//!     let shutdown = client.shutdown().await;
//!     operation?;
//!     shutdown?;
//!     Ok::<_, Box<dyn std::error::Error>>(())
//! })
//! # }
//! # #[cfg(not(unix))]
//! # fn main() {}
//! ```
//!
//! # Scope and lifecycle
//!
//! The default `s3` feature enables S3/S3-compatible storage and the s8k alias.
//! Without default features the library supports core and POSIX on Unix.
//! Byte reads default to a 64 MiB limit. Streams and file APIs keep transfer
//! memory bounded. Streams may fail after opening; consume errors through EOF.
//! Await [`StorageClient::shutdown`] on success and failure while the runtime is
//! alive. Deferred subsystems are rejected in configuration.
//!
pub mod client;
pub mod config;
pub mod credentials;
pub mod error;
mod filter;
pub mod path;
pub mod provider;
pub mod transfer;
pub mod types;

pub use client::StorageClient;
pub use config::{ConfigLoader, ResolvedConfig};
pub use error::{ErrorKind, MscError, Result};
pub use path::ObjectPath;
pub use provider::{ProviderRegistry, StorageProvider};
pub use transfer::*;
pub use types::*;

/// The crate version supplied by Cargo.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
