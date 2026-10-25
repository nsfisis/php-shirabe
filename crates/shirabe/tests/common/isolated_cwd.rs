//! Per-test working directory for integration tests.
//!
//! The working directory is process-wide state, so tests that chdir would otherwise have to be
//! `#[serial]`. On Linux, `unshare(CLONE_FS)` gives the calling thread its own fs struct (cwd,
//! root, umask); libtest runs each test on its own thread, so such tests can run in parallel.
//! Child processes and threads spawned from the test thread afterwards share its fs struct, but
//! threads that already existed (e.g. the workers of the shared tokio runtime in
//! `async_runtime.rs`) keep the cwd of whichever thread spawned them.
//!
//! Other platforms have no per-thread cwd, so tests using this helper must stay serial there:
//! annotate them with `#[cfg_attr(not(target_os = "linux"), serial)]`.
//!
//! Included via `#[path = "common/isolated_cwd.rs"] mod isolated_cwd;`.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

pub struct RestoreCwd(PathBuf);

impl Drop for RestoreCwd {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

/// Changes the working directory of the calling test thread only, restoring it on drop.
///
/// The restore matters under `--test-threads=1`, where libtest runs tests on the main thread and
/// the unshared fs struct carries over from one test to the next.
pub fn set_current_dir(dir: impl AsRef<Path>) -> RestoreCwd {
    #[cfg(target_os = "linux")]
    nix::sched::unshare(nix::sched::CloneFlags::CLONE_FS).unwrap();

    let prev = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir).unwrap();
    RestoreCwd(prev)
}
