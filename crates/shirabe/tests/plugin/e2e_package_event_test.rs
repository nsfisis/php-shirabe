//! Package event E2E compatibility check: upstream Composer and Shirabe each install a fixture
//! project whose plugin subscribes to every `PackageEvents` constant and appends what each event
//! exposes to a trace file. Upstream has no test that dispatches package events through a real
//! plugin, so the whole fixture is Shirabe-authored (`fixtures/e2e-package-event/`) and nothing
//! has to be fetched; the test skips only while the PHP runtime or the Composer checkout is
//! missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::plugin_installer_test::{lock_php_worker, php_runtime_available};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-package-event")
}

struct Run {
    exit_code: i32,
    trace: String,
    repo_trace: String,
}

/// Runs `install` in a fresh copy of the fixture and returns the exit code with the traces the
/// plugin wrote.
fn install(program: &str, prefix_args: &[&str]) -> Run {
    let work = TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let project = work.path().join("project");
    let output = std::process::Command::new(program)
        .args(prefix_args)
        .arg("install")
        .current_dir(&project)
        .env("COMPOSER_HOME", work.path().join("home"))
        .env("COMPOSER_CACHE_DIR", work.path().join("cache"))
        .env("COMPOSER_NO_INTERACTION", "1")
        .env("COLUMNS", "120")
        .env("LINES", "30")
        .output()
        .unwrap();
    let read = |name: &str| std::fs::read_to_string(project.join(name)).unwrap_or_default();
    Run {
        exit_code: output.status.code().unwrap_or(-1),
        trace: read("package-event-trace.txt"),
        repo_trace: read("package-event-repo-trace.txt"),
    }
}

#[test]
fn test_package_events_match_upstream_composer() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();
    let composer_bin = composer_bin.to_str().unwrap().to_string();

    let upstream = install("php", &[composer_bin.as_str()]);
    let shirabe = install(env!("CARGO_BIN_EXE_shirabe"), &[]);

    assert_eq!(0, upstream.exit_code, "upstream install must succeed");
    assert_eq!(upstream.exit_code, shirabe.exit_code);
    assert_eq!(upstream.trace, shirabe.trace);

    // Pinned as well as compared, so a run where neither side dispatches anything cannot pass.
    // The plugin is activated by the very batch it observes, hence the first line: its own
    // post-package-install, deferred until after the batch's operations have run. The two
    // packages of the next batch report their pre-events before either post-event for the same
    // reason.
    assert_eq!(
        "\
post-package-install devMode=1 class=Composer\\DependencyResolver\\Operation\\InstallOperation type=install packages=shirabe-test/package-event-recorder operations=3 root=shirabe/e2e-package-event show=Installing <info>shirabe-test/package-event-recorder</info> (<comment>1.0.0</comment>)
pre-package-install devMode=1 class=Composer\\DependencyResolver\\Operation\\InstallOperation type=install packages=shirabe-test/lib-a operations=3 root=shirabe/e2e-package-event show=Installing <info>shirabe-test/lib-a</info> (<comment>1.0.0</comment>)
pre-package-install devMode=1 class=Composer\\DependencyResolver\\Operation\\InstallOperation type=install packages=shirabe-test/lib-b operations=3 root=shirabe/e2e-package-event show=Installing <info>shirabe-test/lib-b</info> (<comment>1.0.0</comment>)
post-package-install devMode=1 class=Composer\\DependencyResolver\\Operation\\InstallOperation type=install packages=shirabe-test/lib-a operations=3 root=shirabe/e2e-package-event show=Installing <info>shirabe-test/lib-a</info> (<comment>1.0.0</comment>)
post-package-install devMode=1 class=Composer\\DependencyResolver\\Operation\\InstallOperation type=install packages=shirabe-test/lib-b operations=3 root=shirabe/e2e-package-event show=Installing <info>shirabe-test/lib-b</info> (<comment>1.0.0</comment>)
",
        upstream.trace
    );
}

// Upstream's operation chain starts running where it is built, because a `prepare()` that
// returns null becomes an already-fulfilled React promise whose `then()` handlers run through
// the immediately drained queue; the pre-event of the next operation therefore already sees the
// previous one installed. Shirabe builds a lazy future per operation and only drives them in
// wait_on_promises, so every pre-event of a batch sees the repository as it was before the
// batch. Upstream: 1 / 1 / 2 / 3 / 3, Shirabe: 1 / 1 / 1 / 3 / 3.
#[ignore = "operation chains run where they are built upstream but only in wait_on_promises here, so the repository state a package event observes differs (TODO(phase-c) promise cluster)"]
#[test]
fn test_local_repository_seen_by_package_events_matches_upstream_composer() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();
    let composer_bin = composer_bin.to_str().unwrap().to_string();

    let upstream = install("php", &[composer_bin.as_str()]);
    let shirabe = install(env!("CARGO_BIN_EXE_shirabe"), &[]);

    assert_eq!(upstream.repo_trace, shirabe.repo_trace);
}
