//! Minimal synchronous future executor used as a drop-in for the `tokio::runtime::Runtime::new()`
//! + `block_on` sync bridges scattered across the codebase (repository / installer / downloader).
//!
//! Those bridges drive `async fn`s whose `.await` points all resolve synchronously — this relies
//! on the invariant below. `CurlDownloader` no longer qualifies: it now performs real async I/O
//! via a non-blocking `reqwest::Client`, so `HttpDownloader` drives it through its own dedicated
//! `tokio::runtime::Runtime` instead (see `http_downloader::curl_runtime`), not through this
//! module. The other call sites (`file_downloader.rs`, `version_guesser.rs`,
//! `installation_manager.rs`, `composer_repository.rs`, `sync_helper.rs`) still rely on this
//! module because none of their awaited futures actually park on a reactor.
//!
//! Nesting `tokio` runtimes is forbidden ("Cannot start a runtime from within a runtime"), which is
//! why this no-reactor executor exists for those remaining call sites; it can be nested freely.
//!
//! TODO(phase-e): remove this module once the async bridges are either made genuinely synchronous or
//! consolidated onto a single shared runtime driven from `main`.

use std::future::Future;
use std::task::{Context, Poll};

/// Polls `fut` to completion on the current thread without any tokio runtime.
///
/// Relies on the invariant that no awaited future parks on a reactor (it would otherwise spin).
pub fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = std::pin::pin!(fut);
    let waker = std::task::Waker::noop();
    let mut cx = Context::from_waker(waker);
    loop {
        if let Poll::Ready(value) = fut.as_mut().poll(&mut cx) {
            return value;
        }
        std::hint::spin_loop();
    }
}
