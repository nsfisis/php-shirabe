//! Sync-to-async bridge used across the codebase (repository / installer / downloader) at the
//! many call sites that still have a synchronous signature but need to drive an `async fn`.
//!
//! `main.rs` enters a single top-level `tokio::runtime::Runtime` for the whole process, so in
//! production `block_on` below always finds that runtime ambiently available via
//! `Handle::try_current()` and rides it via `tokio::task::block_in_place` — this is the
//! tokio-sanctioned way to call `Handle::block_on` from sync code that may itself already be
//! running inside a task driven by that same runtime (a plain nested `Runtime::block_on` would
//! panic with "Cannot start a runtime from within a runtime"; `block_in_place` does not).
//! Because it rides the real ambient runtime instead of a reactor-less busy-spin, awaited futures
//! that actually need the reactor (timers, real non-blocking socket I/O such as
//! `CurlDownloader::download`) now work correctly here too.
//!
//! Test binaries generally call production sync APIs directly with no ambient runtime entered
//! (see `crates/shirabe/tests/common/async_runtime.rs` for the few that do enter one). For that
//! case — and for any other call site reached before `main.rs`'s runtime exists — `block_on` falls
//! back to a disposable single-threaded runtime scoped to just that one call.
//!
//! TODO(phase-e): this still leaves every one of `block_on`'s call sites synchronous rather than
//! genuinely `async fn` propagated up to `Command::execute`, which remains the end goal of the
//! async re-architecture (see the design doc). Nested `block_on` call sites (a sync fn reached
//! from inside another `block_on`'s async block) do not run concurrently with their siblings —
//! `block_in_place` only prevents panics/hangs there, it does not parallelize them — so real
//! overlap only exists where a call chain is genuinely `async fn`/`.await` end-to-end (as arranged
//! for `ComposerRepository::get_security_advisories`/`load_async_packages`, see
//! `repository/composer_repository.rs`). Closing that gap for the remaining call sites requires
//! the full `async fn` propagation this module was always meant to be replaced by.

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
