##########################
Native Rust API Foundation
##########################

The native package's generated Rust API reference is built with
``just multi-storage-client-rust/document`` and appears at
``multi-storage-client-rust/target/doc/multi_storage_client/index.html``.
The supported subset is documented in ``multi-storage-client-rust/docs/api.md``
and ``specs/000-foundation.md``. Python APIs and PyO3 stubs are unchanged.

``StorageProvider`` is the asynchronous provider-neutral boundary.
``StorageClient`` owns shared client policy and operation retry budgets.
Capabilities describe optional behavior; typed errors determine retry eligibility.
Requested conditions are enforced or explicitly rejected. All keys are relative
to their profile root, and ``base_path`` is applied once.

Supported client operations include named-profile construction, ``info``,
``read``, ``read_range``, ``read_stream``, seekable ``reader``, ``write``, spooled
``writer``, ``upload_from``, ``download_to``, ordered ``list``, exact-key ``delete``,
and explicit ``shutdown``. Self-contained copy, glob, symlink, presigning, and
bounded batch file APIs are retained. Optional provider defaults report
``Unsupported``. Recursive client deletion is explicitly unsupported.

Large file transfers use bounded memory and private disk staging. A failed
download preserves an existing destination. Returned streams can fail after
opening. Cancellation retains ownership of required multipart cleanup;
applications stop issuing operations and await shutdown while Tokio is alive.
