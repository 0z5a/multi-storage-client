# Foundation validation and handoff

Date: 2026-11-10 (America/Los_Angeles).
Branch: `shunjiad/feat-rust-client-foundation`.
Base: freshly fetched upstream main at `9f56d57dfef72ead214d12992cd237a1e563dfad`.
Reference source: `134e2669c36595415f4a65f46b14bbf7ca3e3a2f`.
The reference checkout remains unchanged. Unrelated target untracked files are
preserved.

## Native Rust checks

All requested checks passed, including through the parent Nix environment:

| Check | Result |
|---|---|
| cargo fmt --all -- --check | Passed |
| cargo clippy --locked --all-targets --all-features -- -D warnings | Passed |
| cargo test --locked --all-features --all-targets | 109 passed, 49 skipped by default (emulator or child-process fixtures are exercised separately) |
| cargo test --locked --all-features --doc | 3 passed |
| cargo test --locked --no-default-features --all-targets | 77 passed |
| RUSTDOCFLAGS="-D warnings" cargo doc --locked --all-features --no-deps | Passed |
| just multi-storage-client-rust/build | Passed, including POSIX example and library/binary build |
| just multi-storage-client-rust/test-s3 | 65 passed across native HTTP, listing, public-client, transport reliability, and benchmark suites |
| S3 profile example against isolated Versity | Passed, including 10 MiB multipart object and exact owned-key cleanup |
| just multi-storage-client-rust/smoke | Passed, 10 JSON phases and 30 successful operation results, no failures |
| GitHub actionlint, pipeline YAML and shell syntax | Passed; one native job outside each Python matrix |

The S3 benchmark uses 1 and 2 real processes, 2 threads per process, 3 objects
of 8 MiB, and byte/file upload/download plus delete phases. Its configured 5 MiB
threshold exercises multipart file uploads. Local and remote sentinels survive.
The POSIX smoke uses 1 and 2 processes, 2 threads, and 3 objects of 1 KiB, with
file phases, JSON output, and preservation assertions. A focused worker test
observes active I/O to enforce the configured concurrency bound and partition
counts. Benchmark failure and Ctrl-C tests verify exit codes and cleanup.

Versity has private data and IAM directories, an ephemeral port, and a distinct
executable name. It ran alongside the monorepo Python storage test lifecycle.
The macOS Nix recipe CFLAGS setting avoids transitive libuuid headers shadowing
Apple SDK UUID types. No Nix input, lockfile, or accelerator dependency changed.

## Existing monorepo checks

- Nix formatting, Explorer lint/build and 105 Explorer tests passed.
- Python lint/type checks, 18 accelerator Rust tests, 1,752 Python unit tests
  (3 skipped), and 6 Ray tests passed.
- CPython 3.11 wheels for macOS ARM64, Linux ARM64, and Linux x86-64 built
  successfully. The Python source distribution was built and its 132 archive
  entries verified using an explicit temporary output directory, then copied
  into the package dist directory. All wheel archives passed CRC
  verification and contain the Python accelerator extension, without the native
  Rust subproject being added to Python packaging.
- `just multi-storage-client-docs/build` passed, including lint/type checks and
  Sphinx with warnings treated as errors. The native API reference page is
  present in the generated site. Documentation-site user guide changes were
  deferred on 2026-11-12, and the revised site passed the same build command.
  Native usage guidance remains in the subproject README and docs.
- `just multi-storage-client-scripts/build` passed lint/type checks and 32 tests.
- Root build stage commands returned success after resumption. The initial
  CI-mode `just build` command
  exceeded its ten-minute execution limit during Python wheel packaging.
  Packaging was resumed using the same recipe commands and compiled artifacts;
  the remaining native, documentation, and scripts stages completed separately.
  No passed test suite was rerun just to restart the full root command.
- GitHub and GitLab configuration was checked locally; hosted jobs were not
  triggered during this validation run.

## Remaining qualification

All native checks pass. The root build was completed in stages after its
execution timeout, rather than obtaining one uninterrupted `just build` exit
status. An additional artifact check found that the existing `uv build --sdist`
command reports success but leaves no archive in its default output directory
on this checkout. Using `--out-dir /tmp/msc-rust-foundation-sdist` produced a
valid 132-entry source archive, which was copied into the package dist directory.
The Python packaging configuration and dependency graph were not changed to
address this output-directory behavior.

Local native checks ran on macOS ARM64; hosted Linux pipeline execution and
live-cloud/performance qualification remain subsequent verification.

## Intentional limits

No real-cloud E2E or performance qualification was performed. GCS, Azure,
cache, metadata/manifests, replicas, composites, synchronization, DSS, general
CLI, telemetry, RDMA, bindings, and release publication are deferred. Deferred
configuration is rejected explicitly. Client downloads use bounded atomic
streaming without persistent parallel checkpoints. Recursive deletion is
unsupported. See [the contract](api.md) and [source adaptations](import.md).

## Changed files

Only task files are listed; unrelated untracked files are excluded.

```text
.github/workflows/.source.yml
.gitignore
.gitlab/pipelines/.source.yml
AGENTS.md
README.md
justfile
multi-storage-client-docs/src/index.rst
multi-storage-client-docs/src/references/rust_native.rst
multi-storage-client-rust/Cargo.lock
multi-storage-client-rust/Cargo.toml
multi-storage-client-rust/README.md
multi-storage-client-rust/docs/api.md
multi-storage-client-rust/docs/benchmark.md
multi-storage-client-rust/docs/import.md
multi-storage-client-rust/docs/validation.md
multi-storage-client-rust/examples/benchmark-smoke.json
multi-storage-client-rust/examples/posix.yaml
multi-storage-client-rust/examples/posix_quickstart.rs
multi-storage-client-rust/examples/s3-compatible.yaml
multi-storage-client-rust/examples/s3-versity.yaml
multi-storage-client-rust/examples/s3.yaml
multi-storage-client-rust/examples/s3_profile.rs
multi-storage-client-rust/examples/support/mod.rs
multi-storage-client-rust/justfile
multi-storage-client-rust/rust-toolchain.toml
multi-storage-client-rust/scripts/smoke-posix.sh
multi-storage-client-rust/scripts/test-s3.sh
multi-storage-client-rust/specs/000-foundation.md
multi-storage-client-rust/src/bin/msc-benchmark/config.rs
multi-storage-client-rust/src/bin/msc-benchmark/main.rs
multi-storage-client-rust/src/bin/msc-benchmark/metrics.rs
multi-storage-client-rust/src/bin/msc-benchmark/processes.rs
multi-storage-client-rust/src/bin/msc-benchmark/worker.rs
multi-storage-client-rust/src/client/builder.rs
multi-storage-client-rust/src/client/delete.rs
multi-storage-client-rust/src/client/download.rs
multi-storage-client-rust/src/client/listing.rs
multi-storage-client-rust/src/client/mod.rs
multi-storage-client-rust/src/client/read.rs
multi-storage-client-rust/src/client/reader.rs
multi-storage-client-rust/src/client/write.rs
multi-storage-client-rust/src/client/writer.rs
multi-storage-client-rust/src/config/document.rs
multi-storage-client-rust/src/config/loader.rs
multi-storage-client-rust/src/config/mod.rs
multi-storage-client-rust/src/config/redaction.rs
multi-storage-client-rust/src/config/schema.rs
multi-storage-client-rust/src/config/tests.rs
multi-storage-client-rust/src/config/urls.rs
multi-storage-client-rust/src/config/validation.rs
multi-storage-client-rust/src/credentials.rs
multi-storage-client-rust/src/error.rs
multi-storage-client-rust/src/filter.rs
multi-storage-client-rust/src/lib.rs
multi-storage-client-rust/src/path.rs
multi-storage-client-rust/src/provider/mod.rs
multi-storage-client-rust/src/provider/posix/mod.rs
multi-storage-client-rust/src/provider/posix/unix.rs
multi-storage-client-rust/src/provider/posix/unix/listing.rs
multi-storage-client-rust/src/provider/s3/listing.rs
multi-storage-client-rust/src/provider/s3/listing/delimited.rs
multi-storage-client-rust/src/provider/s3/mod.rs
multi-storage-client-rust/src/provider/s3/native.rs
multi-storage-client-rust/src/provider/s3/options.rs
multi-storage-client-rust/src/provider/s3/resume.rs
multi-storage-client-rust/src/provider/s3/transport.rs
multi-storage-client-rust/src/provider/s3/upload.rs
multi-storage-client-rust/src/provider/sort.rs
multi-storage-client-rust/src/transfer.rs
multi-storage-client-rust/src/types.rs
multi-storage-client-rust/tests/benchmark_cli.rs
multi-storage-client-rust/tests/posix_integration.rs
multi-storage-client-rust/tests/s3_http_errors.rs
multi-storage-client-rust/tests/s3_integration.rs
multi-storage-client-rust/tests/s3_listing_http.rs
multi-storage-client-rust/tests/s3_native_http.rs
multi-storage-client-rust/tests/storage_client_api.rs
```


## PR 248 review follow-up, 2026-11-12

Addressed the 20 inline threads and the 10 additional review-body findings.
The correctness review covered file-handle ownership and retry behavior, POSIX
publication cancellation, symlink containment and copy admission, bounded listing,
S3 page validation and cleanup, credentials, and benchmark disk preflight.
The complexity review removed the separate recursive S3 listing engine,
process-local staging registry, batch-download wrapper/allocation, and unused
multi-client example cleanup flexibility. User guide changes remain deferred.

The complete native `just build` passed with lint, 126 default-feature tests,
89 POSIX-only tests, three doctests, warning-free API documentation, both runnable
examples, 69 S3 HTTP/emulator tests, and the POSIX benchmark smoke. S3 tests and
benchmark smoke are now prerequisites of `build`. The Sphinx build, actionlint,
pipeline YAML parsing, and shell syntax checks also passed. Regression cases cover
spool substitution, explicit buffer overrides, stale credential fallback,
cancelled queued publication, single-permit copies, aliased parents, listing
with one blocking thread, cross-process staging, folder markers, continuation
tokens, listing bounds, duplicate keys, and shared-filesystem disk accounting.

These checks ran on macOS ARM64 through the existing Nix environment. Hosted CI
and real-cloud performance qualification remain separate. Python and accelerator
source and dependency graphs are unchanged by this review follow-up.
