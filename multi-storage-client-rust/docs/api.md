# Supported API contract

| Entry point | Behavior |
|---|---|
| `ConfigLoader::load` | Strict YAML/JSON startup parsing, includes, environment substitution, redaction and provenance |
| `ResolvedConfig::resolve` | Separate profile and relative key from MSC/file/S3 URLs and absolute paths |
| `StorageClient::builder(config, profile).build()` | Construct one physical provider and its retry policy |
| `StorageClient::from_provider` | Use an application-owned adapter |
| `info(key, InfoOptions)` | Metadata, optional relative symlink following, bounded hop/cycle checks |
| `read(key)`, `read_with_options` | EOF-validated byte reads, default 64 MiB limit |
| `read_range(key, range, ReadOptions)` | Half-open offset/length or suffix reads, clipped at EOF |
| `read_stream(key, ReadOptions)` | Bounded stream; opening and later stream errors are separate outcomes |
| `reader(key, ReadOptions)` | Generation-pinned Tokio AsyncRead + AsyncSeek with bounded range buffers |
| `write(key, Bytes, WriteOptions)` | Publish bytes with requested conditions or explicit rejection |
| `writer(key, size_hint, WriteOptions)` | Bounded disk spool, publication only by explicit finish |
| `upload_from(key, file, WriteOptions)` | Replayable private file snapshot, bounded transfer memory |
| `download_to(key, file, ReadOptions)` | Stream, validate, flush/sync, atomic destination replacement |
| `list(ListOptions)`, `list_recursive`, `glob` | Globally ordered metadata; stream errors may follow opening |
| `delete(key, false, DeleteOptions)` | Exact-key idempotent deletion; recursive=true is Unsupported |
| `upload_many`, `download_many`, `delete_many` | Bounded concurrency and original-index partial results |
| `copy`, `make_symlink`, `presigned_url` | Capability-driven optional operations |
| `shutdown()` | Await owned provider cleanup while the application runtime is alive |

`StorageProvider` is the asynchronous provider-neutral I/O boundary. Providers
apply their configured root once. `ObjectPath` validates relative UTF-8 keys and
rejects traversal. POSIX anchors filesystem operations under its configured root.
Capabilities describe optional behavior; default optional trait methods return
Unsupported before mutation. ErrorKind and retryable metadata decide retry
eligibility, with one client policy and disabled SDK operation retries.
Multipart parts share that policy without replaying uncertain completion.

S3 supports custom endpoints, path-style addressing, SwiftStack options, small
PUT and bounded multipart upload. Conditional multipart publication is explicitly
unsupported. User metadata is string-valued on S3; POSIX preserves JSON values.
Backend-scoped version tokens cannot be applied to unrelated backends.

POSIX reserves components beginning with `.msc-tmp-` for staging and omits
them from listings, including staging owned by other processes. Reader buffer
options use `None` to inherit the builder setting and `Some(bytes)` to override it.

File upload snapshots consume temporary disk equal to source size and are part
of API latency. Downloads synchronize the temporary destination before atomic
publication. A failed or cancelled transfer must preserve an existing destination.
Client parallel checkpoints are deferred. Await shutdown after dropped multipart
futures so required abort work remains owned and failures remain observable.
Clients may be used again after shutdown; it drains cleanup rather than closing
admission permanently. Applications must stop issuing operations before their
final shutdown. No runtime, exporter or process-global client is installed.
