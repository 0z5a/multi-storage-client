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

//! Shared fixture and cleanup helpers; the storage workflows remain in each example.
#![allow(dead_code)] // Each example uses only the helpers its workflow needs.

use multi_storage_client::StorageClient;
use std::error::Error;

/// Examples can propagate storage, config, and standard I/O diagnostics.
pub type ExampleResult<T> = Result<T, Box<dyn Error>>;

/// Attempt shutdown even if the workflow failed, preserving the workflow error.
pub async fn finish<T>(result: ExampleResult<T>, client: &StorageClient) -> ExampleResult<T> {
    let shutdown = client.shutdown().await;
    result.and_then(|value| shutdown.map(|()| value).map_err(Into::into))
}
