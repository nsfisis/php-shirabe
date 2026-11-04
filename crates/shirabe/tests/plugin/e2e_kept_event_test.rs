//! Kept event E2E compatibility check: upstream Composer and Shirabe each install a fixture project
//! whose plugin and `scripts` handler keep the event objects they receive and use them after the
//! call that delivered them has returned, appending what they observe to a trace file. Upstream has
//! no test that keeps an event past its listener call, so the whole fixture is Shirabe-authored
//! (`fixtures/e2e-kept-event/`) and nothing has to be fetched; the test skips only while the PHP
//! runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::{lock_php_worker, php_runtime_available};
use bstr::{BStr, BString};
use shirabe_php_shim::os_str_from_bytes;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-kept-event")
}

struct Run {
    exit_code: i32,
    trace: BString,
}

/// Runs `install` in a fresh copy of the fixture and returns the exit code with the plugin's
/// trace.
fn install(program: &BStr, prefix_args: &[&OsStr]) -> Run {
    let work = TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let project = work.path().join("project");
    let output = std::process::Command::new(os_str_from_bytes(&program))
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
    Run {
        exit_code: output.status.code().unwrap_or(-1),
        trace: BString::from(
            std::fs::read(project.join("kept-event-trace.txt")).unwrap_or_default(),
        ),
    }
}

#[test]
fn test_events_kept_past_their_listener_call_match_upstream_composer() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();

    let upstream = install(BStr::new("php"), &[composer_bin.as_os_str()]);
    let shirabe = install(BStr::new(env!("CARGO_BIN_EXE_shirabe")), &[]);

    assert_eq!(0, upstream.exit_code, "upstream install must succeed");
    assert_eq!(upstream.exit_code, shirabe.exit_code);
    assert_eq!(upstream.trace, shirabe.trace);

    // Pinned as well as compared, so a run where neither side wrote a trace cannot pass. The
    // listeners of one dispatch receive the very same object, so stopping propagation through the
    // one an earlier listener kept skips the third listener for lib-a; every event kept from an
    // earlier dispatch stays readable and changeable until the post-command event.
    assert_eq!(
        "\
keep post-package-install shirabe-test/kept-event-keeper
second shirabe-test/kept-event-keeper identical=true
third shirabe-test/kept-event-keeper
keep post-package-install shirabe-test/lib-a
second shirabe-test/lib-a identical=true
stopped through the kept event: true
keep post-package-install shirabe-test/lib-b
second shirabe-test/lib-b identical=true
third shirabe-test/lib-b
keep post-autoload-dump
script keep post-autoload-dump identical to the plugin-kept one=true
kept[0] name=post-package-install package=shirabe-test/kept-event-keeper stopped=false devMode=true operations=3 root=shirabe/e2e-kept-event
kept[1] name=post-package-install package=shirabe-test/lib-a stopped=true devMode=true operations=3 root=shirabe/e2e-kept-event
kept[2] name=post-package-install package=shirabe-test/lib-b stopped=false devMode=true operations=3 root=shirabe/e2e-kept-event
kept[0] stopped after stopPropagation=true
kept[0] identical to kept[1]=false
script kept name=post-autoload-dump flags={\"optimize\":false} arguments=[] devMode=true stopped=false identical to the plugin-kept one=true identical to the current one=false same io=true
",
        upstream.trace
    );
}
