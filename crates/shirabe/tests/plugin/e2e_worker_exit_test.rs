//! PHP process death E2E compatibility check: a fixture plugin ends the PHP process from a
//! listener, by `exit()`, by a fatal error and by a signal. Under Composer that process is
//! Composer itself; under Shirabe it is the PHP worker, and Shirabe has to end the same way. The
//! whole fixture is Shirabe-authored (`fixtures/e2e-worker-exit/`), so nothing has to be fetched;
//! the test skips only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::{lock_php_worker, php_runtime_available};
use std::os::unix::process::ExitStatusExt as _;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-worker-exit")
}

/// Runs `install` in a fresh copy of the fixture with the plugin ending the process as `how`
/// names, and returns how the process ended: its exit code, or the signal that killed it.
fn install(program: &str, prefix_args: &[&str], how: &str) -> (Option<i32>, Option<i32>) {
    let work = TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let output = std::process::Command::new(program)
        .args(prefix_args)
        .arg("install")
        .current_dir(work.path().join("project"))
        .env("COMPOSER_HOME", work.path().join("home"))
        .env("COMPOSER_CACHE_DIR", work.path().join("cache"))
        .env("COMPOSER_NO_INTERACTION", "1")
        .env("SHIRABE_E2E_WORKER_EXIT", how)
        .output()
        .unwrap();
    (output.status.code(), output.status.signal())
}

#[test]
fn test_shirabe_ends_the_way_the_php_process_does() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();
    let composer_bin = composer_bin.to_str().unwrap().to_string();

    let ended = |how: &str| {
        (
            install("php", &[composer_bin.as_str()], how),
            install(env!("CARGO_BIN_EXE_shirabe"), &[], how),
        )
    };

    let (upstream, shirabe) = ended("exit");
    assert_eq!((Some(3), None), upstream);
    assert_eq!(upstream, shirabe);

    let (upstream, shirabe) = ended("fatal");
    assert_eq!((Some(255), None), upstream);
    assert_eq!(upstream, shirabe);

    let (upstream, shirabe) = ended("kill");
    assert_eq!((None, Some(9)), upstream);
    assert_eq!(upstream, shirabe);
}
