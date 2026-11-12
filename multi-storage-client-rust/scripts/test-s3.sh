#!/usr/bin/env bash
# SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
# http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Starts only the S3 emulator used by the upstream Python client's tests.
set -euo pipefail

emulator="${VERSITYGW_BIN:-versitygw}"
if ! command -v "$emulator" >/dev/null 2>&1; then
    echo 'versitygw is required; run this recipe inside nix develop.' >&2
    exit 1
fi

sandbox=$(mktemp -d "${TMPDIR:-/tmp}/msc-rust-s3.XXXXXX")
emulator_pid=''
cleanup() {
    if [[ -n "$emulator_pid" ]]; then
        kill "$emulator_pid" 2>/dev/null || true
        wait "$emulator_pid" 2>/dev/null || true
    fi
    rm -rf "$sandbox"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

port="${MSC_S3_TEST_PORT:-$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')}"
export MSC_S3_TEST_ENDPOINT="http://127.0.0.1:$port"
export MSC_S3_TEST_BUCKET='msc-rust-tests'
export MSC_S3_TEST_ACCESS_KEY='root-access-key'
export MSC_S3_TEST_SECRET_KEY='root-secret-key'
export MSC_S3_TEST_SANDBOX="$sandbox/data"
mkdir -p "$sandbox/data" "$sandbox/iam"

# The Python test cleanup selects processes by the versitygw executable name.
cp "$(command -v "$emulator")" "$sandbox/msc-rust-s3"
"$sandbox/msc-rust-s3" --port "127.0.0.1:$port" --access "$MSC_S3_TEST_ACCESS_KEY" \
    --secret "$MSC_S3_TEST_SECRET_KEY" --region us-east-1 --health /health \
    --iam-dir "$sandbox/iam" --quiet posix "$sandbox/data" \
    >"$sandbox/emulator.log" 2>&1 &
emulator_pid=$!

ready=false
for _ in {1..100}; do
    if ! kill -0 "$emulator_pid" 2>/dev/null; then
        cat "$sandbox/emulator.log" >&2
        exit 1
    fi
    if curl --fail --silent --output /dev/null "$MSC_S3_TEST_ENDPOINT/health"; then
        ready=true
        break
    fi
    sleep 0.1
done
if [[ "$ready" != true ]]; then
    echo 'S3 emulator did not become ready within 10 seconds.' >&2
    cat "$sandbox/emulator.log" >&2
    exit 1
fi

curl --fail --silent --show-error --output /dev/null --request PUT \
    --aws-sigv4 'aws:amz:us-east-1:s3' \
    --user "$MSC_S3_TEST_ACCESS_KEY:$MSC_S3_TEST_SECRET_KEY" \
    "$MSC_S3_TEST_ENDPOINT/$MSC_S3_TEST_BUCKET"

cargo test --locked --all-features --test s3_native_http --test s3_listing_http -- --test-threads=1 "$@"
cargo test --locked --all-features --test s3_integration --test s3_http_errors \
    --test benchmark_cli -- --ignored --skip endpoint_environment_child_probe \
    --test-threads=1 "$@"

cargo run --locked --example s3_profile -- examples/s3-versity.yaml emulator
