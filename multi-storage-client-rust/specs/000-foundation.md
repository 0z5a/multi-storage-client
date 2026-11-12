# Native Rust MSC foundation

## Summary and provenance

Import the POSIX/S3 subset of the Rust reference at
`134e2669c36595415f4a65f46b14bbf7ca3e3a2f` (Shunjia Ding,
`feat: close MSC 1.4 core parity gaps`). The source checkout is read-only.
This package follows the parent repository's Apache-2.0 license.

## API and architecture contract

Applications own and configure their Tokio runtime. `StorageProvider` is the
asynchronous, provider-neutral I/O boundary. `ProviderRegistry` constructs
built-in `file`, `s3`, and `s8k` adapters or application-owned factories.
`StorageClient::builder(config, profile).build().await` selects a named profile.
Client clones share the provider, connection pools, and cleanup owners.

The client provides info, byte/range/suffix/streaming reads, seekable readers,
byte writes, spooled writers, upload_from, download_to, ordered listings,
exact-key deletion, copy, symlinks, presigning, and bounded batch file operations.
Optional provider operations retain documented Unsupported defaults. Capabilities
describe optional behavior; requested conditions are enforced or explicitly
rejected before mutation. Errors carry typed kinds and retry eligibility.

The client owns operation retry policy. SDK retries are disabled. Reads retry
safe operations; writes retry only with safe destination conditions. Multipart
parts use the current client budget, without replaying ambiguous publication.
S3 transport recovery, timeout handling, connection reuse, and abort ownership
are retained from the reference. Returned streams can fail after opening;
consumers must reach EOF and propagate each item error. Streams are not replayed
by the client after being returned to the application.

Keys are profile-relative. Providers apply base_path once. ObjectPath rejects
traversal. POSIX uses anchored directory descriptors and rejects root escapes,
including symlink escapes. Large file transfers have bounded memory. Uploads
create a private replayable disk snapshot and validate source size/modification;
downloads stream to a temporary sibling, validate length, synchronize the file,
and atomically publish. Failed downloads preserve existing destinations.
Cancellation must retain required cleanup ownership. Await explicit shutdown
while the runtime is alive, on both success and failure, to drain multipart
cleanup and report failures. Shutdown does not prevent subsequent operations.

## Scope and intentional adaptations

S3 is the only default Cargo feature. No-default-features supports core and POSIX
on Unix. One independent package, lockfile, and pinned toolchain reuse the parent
Nix environment. There is no Cargo workspace or Python binding change.

Client cache, metadata, replicas, composites, sync, GCS, Azure, general msc CLI,
telemetry, RDMA, bindings, and release publication are deferred. Configurations
requesting those behaviors fail explicitly. Factory and capability boundaries
allow future adapters without importing those runtimes now.

The reference client parallel-download checkpoint implementation depends on
cache filesystem state. This subset uses its bounded streaming atomic-download
path for all file downloads, without persistent checkpoints. This is an
intentional behavior change. Provider-level S3 range recovery is retained.
Cache read overrides and staged metadata options are removed from public types.
Recursive client deletion is explicitly unsupported; exact-key deletion remains
idempotent. Benchmark metadata commit and telemetry phases are removed.

## Backward compatibility and cross-language impact

This is additive to the monorepo. Python APIs, configuration, packaging, PyO3
accelerator dependencies, and .pyi stubs do not change. Native configuration
shares supported profile field spellings with Python, but rejects deferred ones.
Real-cloud E2E and performance qualification remain subsequent work.

## File selection and adaptation plan

| Files | Selection/adaptation |
|---|---|
| src/{error,path,types,credentials}.rs | Import neutral contracts and credentials; trim deferred read options |
| src/provider/{mod.rs,posix/,s3/} | Import adapters and reliability code; remove other backend factories and telemetry file wrapper |
| src/provider/sort.rs | Extract bounded external metadata sorter from reference sync/spool.rs for provider listings |
| src/config/ | Retain discovery, includes, expansion, provenance, strict schemas, URL resolution, redaction; reject deferred fields |
| src/client/ | Keep retry policy and self-contained byte/file/listing helpers; remove decorators and checkpoint state |
| src/bin/msc-benchmark/ | Keep process protocol, bounded concurrency, phases and reporting; remove commits/exporters |
| tests/, examples/ | Select relevant reference contracts and HTTP fixtures; add scope/cleanup regression coverage where missing |
| Cargo.*, rust-toolchain.toml, justfile | Independent package, S3 feature, locked checks and isolated emulator |
| parent justfile, CI source pipelines, README, AGENTS.md, Sphinx reference | Dispatch Rust checks once per pipeline and distinguish accelerator from native library |

## Performance and benchmark contract

No transport tuning is part of this import. Files use bounded chunks and disk
staging; byte APIs and benchmark payloads materialize configured objects.
Benchmark setup/payload generation is outside transfer timing. Successful bytes
are counted after completion only. Reads reach EOF and validate length. File
phases measure snapshot creation and destination synchronization as API behavior.
Process counts mean child processes; per-process operations are bounded by
threads. Unique run namespaces own exact cleanup keys; unrelated data survives.
Failures, including cleanup failures, yield nonzero exit status.

## Acceptance and validation

Run formatting, all-target/all-feature clippy, all-target tests, doctests,
no-default-feature tests, warning-free rustdoc, isolated Versity HTTP/integration
tests, and small POSIX/S3 benchmarks with file phases and JSON output. Verify
cleanup preserves unrelated data. Build Sphinx and check existing Python
packaging through the repository's tooling. Emulator results do not imply
live-cloud qualification.
