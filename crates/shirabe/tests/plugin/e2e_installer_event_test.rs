//! Installer event E2E compatibility check: upstream Composer and Shirabe each install a fixture
//! project and then require one more package, with a plugin subscribed to
//! `InstallerEvents::PRE_OPERATIONS_EXEC` appending what the event exposes to a trace file.
//! Upstream has no test that dispatches installer events through a real plugin, so the whole
//! fixture is Shirabe-authored (`fixtures/e2e-installer-event/`) and nothing has to be fetched;
//! the test skips only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::lock_php_worker;
use bstr::{BStr, BString};
use shirabe_php_shim::os_str_from_bytes;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-installer-event")
}

struct Run {
    exit_codes: Vec<i32>,
    trace: BString,
}

/// Runs `install` and then `require shirabe-test/lib-b:1.0.0` in a fresh copy of the fixture: the
/// plugin is only active once the first command has installed it, so the event it records is the
/// one the second command dispatches.
fn install_then_require(program: &BStr, prefix_args: &[&OsStr]) -> Run {
    let work = TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let project = work.path().join("project");
    let commands: [&[&BStr]; 2] = [
        &[BStr::new("install")],
        &[BStr::new("require"), BStr::new("shirabe-test/lib-b:1.0.0")],
    ];
    let exit_codes = commands
        .iter()
        .map(|args| {
            std::process::Command::new(os_str_from_bytes(&program))
                .args(prefix_args)
                .args(args.iter().map(os_str_from_bytes))
                .current_dir(&project)
                .env("COMPOSER_HOME", work.path().join("home"))
                .env("COMPOSER_CACHE_DIR", work.path().join("cache"))
                .env("COMPOSER_NO_INTERACTION", "1")
                .env("COLUMNS", "120")
                .env("LINES", "30")
                .output()
                .unwrap()
                .status
                .code()
                .unwrap_or(-1)
        })
        .collect();
    Run {
        exit_codes,
        trace: BString::from(
            std::fs::read(project.join("installer-event-trace.txt")).unwrap_or_default(),
        ),
    }
}

#[test]
fn test_installer_events_match_upstream_composer() {
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();

    let upstream = install_then_require(BStr::new("php"), &[composer_bin.as_os_str()]);
    let shirabe = install_then_require(BStr::new(env!("CARGO_BIN_EXE_shirabe")), &[]);

    assert_eq!(
        vec![0, 0],
        upstream.exit_codes,
        "upstream commands must succeed"
    );
    assert_eq!(upstream.exit_codes, shirabe.exit_codes);
    assert_eq!(upstream.trace, shirabe.trace);

    // Pinned as well as compared, so a run where neither side dispatches anything cannot pass.
    // The listener after the one stopping propagation never runs.
    assert_eq!(
        "\
pre-operations-exec class=Composer\\Installer\\InstallerEvent devMode=1 executing=1 transaction=Composer\\DependencyResolver\\LocalRepoTransaction operations=1 identical=1 root=shirabe/e2e-installer-event io=1 shown=Composer\\DependencyResolver\\Operation\\InstallOperation:Installing <info>shirabe-test/lib-b</info> (<comment>1.0.0</comment>)
stop 0
stopped 1
",
        upstream.trace
    );
}
