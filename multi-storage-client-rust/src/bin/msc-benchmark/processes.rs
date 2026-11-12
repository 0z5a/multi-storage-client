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

use futures::future::join_all;
use std::{path::Path, process::Stdio, time::Instant};
use tokio::{
    io::{AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

use super::{
    Result,
    metrics::Measurements,
    worker::{self, Job, Phase},
};

struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

pub(super) struct Workers(Vec<Worker>);

impl Workers {
    pub async fn start(job: &Job, objects: usize, directory: &Path) -> Result<Self> {
        let executable = std::env::current_exe()?;
        let active = job.processes.min(objects);
        let mut workers = Vec::with_capacity(active);
        let batch = objects / active;
        let extra = objects % active;
        let mut start = 0;
        for index in 0..active {
            let mut partition = job.clone();
            partition.start = start;
            start += batch + usize::from(index < extra);
            partition.end = start;
            let path = directory.join(format!("worker-{index}.json"));
            tokio::fs::write(&path, serde_json::to_vec(&partition)?).await?;
            let mut child = Command::new(&executable)
                .arg("--worker-job")
                .arg(&path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .kill_on_drop(true)
                .spawn()?;
            let input = child.stdin.take().ok_or("worker stdin unavailable")?;
            let output = BufReader::new(child.stdout.take().ok_or("worker stdout unavailable")?);
            workers.push(Worker {
                child,
                input,
                output,
            });
        }
        let ready: Vec<Result<()>> = join_all(workers.iter_mut().map(|worker| async {
            let response: std::result::Result<(), String> =
                worker::receive(&mut worker.output).await?;
            response.map_err(Into::into)
        }))
        .await;
        for result in ready {
            result?;
        }
        Ok(Self(workers))
    }

    pub async fn phase(&mut self, phase: Phase) -> Result<(Measurements, f64)> {
        let start = Instant::now();
        for worker in &mut self.0 {
            worker::send(&mut worker.input, &phase).await?;
        }
        let responses: Vec<Result<Measurements>> =
            join_all(self.0.iter_mut().map(|worker| async {
                let response: std::result::Result<Measurements, String> =
                    worker::receive(&mut worker.output).await?;
                response.map_err(Into::into)
            }))
            .await;
        let elapsed = start.elapsed().as_secs_f64();
        let mut measurements = Measurements::default();
        for response in responses {
            measurements.merge(response?);
        }
        Ok((measurements, elapsed))
    }

    pub async fn stop(mut self) -> Result<()> {
        let results = join_all(self.0.iter_mut().map(|worker| async {
            worker::send(&mut worker.input, &Phase::Stop).await?;
            worker.input.shutdown().await?;
            let status = worker.child.wait().await?;
            if !status.success() {
                return Err(format!("benchmark worker exited with {status}").into());
            }
            Ok(())
        }))
        .await;
        let mut error = None;
        for result in results {
            if let Err(cause) = result {
                error.get_or_insert(cause);
            }
        }
        error.map_or(Ok(()), Err)
    }
}
