//! Shared tokio runtime bridge for integration tests.
//!
//! `cargo test` runs `#[test]` fns concurrently on separate OS threads, so a per-call
//! `current_thread` Runtime would need one instance per caller. Instead this holds a single
//! process-wide `multi_thread` Runtime that tolerates concurrent `block_on()` callers, and every
//! test bridges into async code through it.
//!
//! Included into each integration-test binary via
//! `#[path = "../common/async_runtime.rs"] mod async_runtime;`.
#![allow(dead_code)]

use std::sync::LazyLock;

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
});

pub fn run<F: std::future::Future>(future: F) -> F::Output {
    RUNTIME.block_on(future)
}
