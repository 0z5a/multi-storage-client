# Source selection and attribution

Primary source: the Rust MSC reference repository, revision
`134e2669c36595415f4a65f46b14bbf7ca3e3a2f`, authored by Shunjia Ding,
`feat: close MSC 1.4 core parity gaps`. The source was inspected read-only.
The imported implementation follows this repository's Apache-2.0 license in
`../LICENSE`; package metadata declares `Apache-2.0` and `publish = false`.

Imported areas: neutral types/errors/paths/credentials, config document loading,
POSIX and object_store-based S3 adapters, bounded metadata sorting, self-contained
client read/write/listing helpers, benchmark process orchestration and reports,
and selected POSIX/client/S3/HTTP/benchmark tests and examples.

Adaptations: remove cache, metadata, replicas, composites, telemetry, other cloud
adapters, general CLI, and sync construction. Extract `sync/spool.rs` as private
`provider/sort.rs` because ordered provider listings require bounded external
sorting. It retains no sync planning/execution behavior. Remove cache controls
and pending metadata controls from public option types. Return native Tokio
POSIX files without the telemetry descriptor wrapper. Reject deferred config
fields before constructing clients or creating benchmark payloads.

Client downloads use the reference's atomic bounded streaming implementation
for every size. Persistent parallel download checkpoints require cache filesystem
code and are deferred. S3 transport-level range recovery is unchanged. Recursive
client deletion is rejected; exact-key deletion and batch exact-key deletion
remain available. Benchmark commit/exporter phases are removed. Existing source
dependency pins and lock resolutions are retained for selected dependencies;
unused dependencies are removed. No transport tuning was performed.

Reference design areas reviewed: specs 000 through 004, core modules,
configuration, client, POSIX/S3 providers, benchmark sources and guide, and
corresponding tests/examples. The implemented contract is in
[000-foundation.md](../specs/000-foundation.md), including the file-level plan.


## Research summary

The reference remains the primary implementation source. Its object_store
adapter separates provider primitives from client policy and carries explicit
multipart cleanup ownership. The upstream
[MultipartUpload contract](https://docs.rs/object_store/latest/object_store/trait.MultipartUpload.html)
states that S3 cannot clean up uploaded parts merely by dropping an upload.
That supports retaining the reference abort owner and explicit shutdown.
[Upstream release notes](https://github.com/apache/arrow-rs-object-store/releases)
include multipart part-retry changes, so the import keeps the tested 0.14.2 pin
and existing retry ownership rather than performing an SDK migration.

The recommendation is a selective import of the existing adapters and a narrow
client policy layer. Main risks are hidden decorator dependencies, accidentally
multiplying retries, cancellation losing upload ownership, and benchmarks
miscounting partial transfers. Selected tests exercise these boundaries. No new
transport abstraction or tuning was introduced.
