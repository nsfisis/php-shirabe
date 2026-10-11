//! Sync-to-async bridge for the synchronous APIs that sit below the `async fn` chain.
//!
//! `main.rs` drives `shirabe::run` with `Runtime::block_on`, and that chain is `async fn` down
//! through `Application::run`, `Command::execute`, `Installer::run`, `InstallationManager::execute`
//! and `Loop::wait`, so the downloads of an install batch are awaited by their caller and overlap
//! on the runtime. `block_on` below serves what is still synchronous under that chain:
//!
//! * `HttpDownloader::get` / `copy`, synchronous in PHP as well (they run `wait()` on their own job)
//! * the fan-outs of `ComposerRepository` (`load_async_packages`, `get_security_advisories`),
//!   reached through the synchronous repository API by `PoolBuilder` and `Problem`
//! * the parallel diffs of `VersionGuesser::guess_feature_version`
//! * the RPC handlers that answer plugin code (`job_registry::tick`, `poll_once`,
//!   `run_worker_reverse_command`), whose requests are synchronous
//!
//! With the process-wide runtime of `main.rs` ambient, `block_on` rides it via
//! `tokio::task::block_in_place` — the tokio-sanctioned way to call `Handle::block_on` from sync
//! code that is itself running inside a future driven by that same runtime (a plain nested
//! `Runtime::block_on` would panic with "Cannot start a runtime from within a runtime";
//! `block_in_place` does not). The awaited future gets the real reactor, so timers and
//! non-blocking socket I/O work inside it.
//!
//! Test binaries generally call production sync APIs with no ambient runtime entered (see
//! `crates/shirabe/tests/common/async_runtime.rs` for the few that do enter one). For that case —
//! and for any other call site reached before `main.rs`'s runtime exists — `block_on` falls back
//! to a disposable runtime scoped to just that one call.
//!
//! TODO(async): each call site above is a synchronous caller driving a future to completion on
//! its own thread. While it runs, the sibling futures of an enclosing `FuturesUnordered` (a
//! `Loop::wait` further up the stack) are not polled, so a fan-out reached this way does not
//! overlap with the group it is called from. Awaiting it from the caller instead needs the
//! repository API (`RepositoryInterface` and its consumers `PoolBuilder` / `Problem`) and
//! `VersionGuesser`'s callers (`RootPackageLoader`) to become `async fn`.

use std::future::Future;

/// Drives `fut` to completion, riding the ambient tokio runtime if one is entered (the normal
/// case once `main.rs` has started), or a disposable one-off runtime otherwise.
///
/// The fallback runtime is `multi_thread` (with a single worker), not `current_thread`: a nested
/// `block_on` call reached from inside `fut` (e.g. a sync fn deep in the same call chain hitting
/// this function again) needs `block_in_place`, which panics on a `current_thread` runtime.
pub fn block_on<F: Future>(fut: F) -> F::Output {
    match tokio::runtime::Handle::try_current() {
        Ok(_) => tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(fut)),
        Err(_) => tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("failed to build a fallback runtime for sync_executor::block_on")
            .block_on(fut),
    }
}
