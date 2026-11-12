# Native Rust Multi-Storage Client

An asynchronous Rust library and `msc-benchmark` executable for POSIX and
S3/S3-compatible storage, including SwiftStack (`s8k`). This is an initial
foundation, with focused emulator coverage. Real-cloud E2E and performance
qualification remain subsequent work. Publication is disabled.

This package is separate from `../multi-storage-client/rust`, the Python PyO3
accelerator. It does not change Python APIs, bindings, configuration or packaging.

## Build and verify

Use the parent repository's existing Nix environment and flake.lock:

```shell
# From the repository root:
nix develop
just multi-storage-client-rust/build
just multi-storage-client-rust/test-s3
just multi-storage-client-rust/smoke
```

Cargo recipes execute from this directory to select its pinned Rust toolchain
and Cargo.lock. There is no repository-wide Cargo workspace.
On macOS inside Nix, recipes prefer SDK C headers to transitive libuuid headers. For a POSIX-only
build, run `cargo build --locked --no-default-features` from this directory.
POSIX requires Unix. The only default Cargo feature is `s3`.

## Library

Applications own the Tokio runtime. Build and share one client per profile:

```rust,no_run
use bytes::Bytes;
use multi_storage_client::{ConfigLoader, StorageClient, WriteOptions};

async fn example() -> Result<(), Box<dyn std::error::Error>> {
let config = ConfigLoader::new().with_explicit_paths(["config.yaml"]).load()?;
let client = StorageClient::builder(config, "data").build().await?;
let result = client.write("hello.txt", Bytes::from_static(b"hello"), WriteOptions::default()).await;
let shutdown = client.shutdown().await;
result?;
shutdown?;
Ok(())
}
```

Every operation takes a profile-relative key. Providers apply `base_path` once.
`ConfigLoader` supports JSON/YAML, environment expansion, ordered includes,
redacted diagnostics, profile URLs, and path mappings. Minimal configurations
are in [examples/posix.yaml](examples/posix.yaml) and
[examples/s3.yaml](examples/s3.yaml), plus
[S3-compatible configuration](examples/s3-compatible.yaml). S3 configuration requires an existing bucket;
credentials may be static, refreshed from a file, or discovered through AWS.
Set `type: s8k` for SwiftStack MIME inference and the same S3 transport.

Use `read` for small objects, `read_range` for bounded ranges, `read_stream` or
`reader` for larger reads, and `upload_from`/`download_to` for files. Consume
streams through EOF and propagate errors after opening. File uploads snapshot
the source to disk; downloads validate and synchronize a temporary sibling before
atomic publication. Memory is bounded independently of file length.
POSIX reserves path components starting with `.msc-tmp-` for private staging,
and excludes them from listings across processes. `writer` spools input and publishes only after `finish().await`.

`list` returns ordered metadata; `delete(key, false, options)` deletes an exact
key idempotently. Recursive deletion is explicitly unsupported. Copy, symlinks,
presigning, and bounded batch file operations are also available where supported.
Check provider capabilities and inspect partial batch failures. Conditions are
enforced or rejected explicitly. Await `shutdown` on success and failure while
the runtime remains alive to finish multipart cleanup.

## Scope

Implemented: neutral errors/types/paths, credentials, strict configuration,
minimal client policy, POSIX, S3/S3-compatible adapters, and process-based
benchmarks. Deferred configuration fails with actionable diagnostics.

Deferred: GCS, Azure, cache, manifests/metadata, bundles, replicas, composites,
sync, DSS, general `msc` CLI, telemetry, RDMA, bindings, release packaging, and
crates.io publication. Client checkpointed parallel downloads are deferred;
S3 transport recovery remains implemented. No private source checkout is needed.

See the [API contract](docs/api.md), [architecture specification](specs/000-foundation.md),
[benchmark guide](docs/benchmark.md), and [source attribution](docs/import.md).
