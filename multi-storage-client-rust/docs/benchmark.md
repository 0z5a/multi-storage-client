# Storage benchmarks

`msc-benchmark` is the Rust counterpart of Python MSC's `msc-benchmark`. It uses
the Rust `StorageClient` for byte uploads/downloads and optional file transfers.
Build and measure the optimized binary:

```shell
cargo build --locked --release --bin msc-benchmark
target/release/msc-benchmark \
  --profile data --storage-config config.yaml \
  --config examples/benchmark-smoke.json --prefix benchmark
```

`--config` selects the benchmark JSON, matching the Python command. The MSC
storage configuration is separate: use `--storage-config`, `MSC_CONFIG`, or the
normal MSC discovery paths. `--profile` is required. Supported providers follow
the Rust project's scope: POSIX on Unix and S3/S3-compatible storage. S3 requires the `s3` Cargo feature.

For a local run with the implicit filesystem profile:

```shell
target/release/msc-benchmark --profile __filesystem__ \
  --prefix tmp/msc-benchmark --config examples/benchmark-smoke.json
```

The benchmark configuration has three required fields and rejects unknown keys:

```json
{
  "processes": [1, 2],
  "threads": [1, 4],
  "test_object_sizes": {"1MB": 16, "64MB": 4}
}
```

Arrays must be nonempty and contain positive integers; object counts must be
positive. Size strings use positive integers and `KB`, `MB`, `GB`, `TB`, or `PB`,
with powers of 1024, matching Python MSC. Every size/process/thread combination
runs upload, download, optional upload_file/download_file, and delete phases.
When `--config` is omitted, the Python defaults apply: 8 processes, 4 threads,
12,800 objects of 4 MiB and 800 objects of 64 MiB. Each size transfers 50 GiB in
each upload/download phase. Use the small example configuration for a first run.

Each active process owns a persistent MSC client and a Tokio runtime with the
requested number of threads. The number of concurrent operations per process is
bounded by `threads`. Object ranges are partitioned without gaps or duplication;
if there are fewer objects than requested processes, only processes with work
are started. Workers stay alive across phases so connection pools retain their normal behavior. File creation, random payload generation,
and client startup are outside the timed phases.

Add file tests with:

```shell
target/release/msc-benchmark --profile data --storage-config config.yaml \
  --config examples/benchmark-smoke.json --prefix benchmark \
  --include-file-tests --file-tests-dir /tmp/msc-files
```

File tests call `upload_from` and `download_to`, including the client's normal
upload snapshot and atomic download behavior. The command generates one local
file per object. It checks that generated files and the shared payload consume
at most 90% of available space before creation. The preflight check aggregates
payloads, file fixtures, POSIX destinations, and concurrent upload/download
staging by filesystem. Each process retains one payload in memory;
concurrent byte downloads need additional memory proportional to object size and
concurrency. The client's byte-read limit is set to the configured object size.

Each invocation stores remote objects beneath a unique
`<prefix>/msc-benchmark-<UUID>` directory and prints its location to stderr.
Memory and file uploads use distinct object names. Cleanup deletes only those
generated keys, including after reported transfer errors.
Local files and worker requests live in a temporary subdirectory; existing
contents of `--file-tests-dir` are preserved. A failed cleanup or forcibly
terminated process can leave remote objects in the printed directory. Ctrl-C
waits for the current phase and then attempts cleanup; it returns exit code 130.

Text output reports successful and failed operations, successful bytes,
elapsed time, throughput, average latency, and P50/P90/P99 latency. Latencies
describe successful operations only; failures contribute zero transferred bytes
and cause exit code 1. P50 is the median; P90/P99 use nearest ranks. All support a single sample;
with no successful operations, latency values are unavailable. Phase elapsed
time uses a monotonic clock and includes coordination and task scheduling.
Delete results describe cleanup of each object's memory/file key pair and have
zero transferred bytes. The OS page cache can affect download measurements.

`--format json` writes one JSON document to stdout, with `profile`, the actual
remote `prefix`, `cancelled`, and `results`. Each result identifies the operation,
size, requested process/thread counts, success/failure counts, successful bytes,
elapsed seconds, bytes/operations per second, and latency values in milliseconds.
Progress and errors go to stderr. Exit codes are 0 for success, 1 for
configuration/operation failure, 2 for CLI argument errors, and 130 for Ctrl-C.


## Smoke verification

From the repository root, run `just multi-storage-client-rust/smoke` for an
isolated POSIX run with file phases, JSON output, and unrelated-data preservation
checks. Run `just multi-storage-client-rust/test-s3` for the isolated Versity
suite, including a small S3 benchmark. Tests allocate a private data directory
and an ephemeral port. The executable has a distinct process name so Python
cleanup cannot select it; the harness stops only the process it started. They can run
alongside the Python emulator. No real-cloud qualification is implied.

Always use the explicit Rust binary path or `cargo run --bin msc-benchmark`.
The monorepo's Python command has the same executable name. Ordinary checks use
`examples/benchmark-smoke.json`; they never invoke the large default workload.
File phase latency includes upload snapshots and destination synchronization.
Byte reads consume through EOF and validate length before counting successful
bytes. Cleanup failures also return nonzero status.
