//! ProcessExecutor E2E compatibility check: upstream Composer and Shirabe each install a fixture
//! project whose plugin builds its own `ProcessExecutor` and writes what every call on it reports
//! to a trace file. Upstream has no test that drives a process executor from plugin code, so the
//! whole fixture is Shirabe-authored (`fixtures/e2e-process-executor/`) and nothing has to be
//! fetched; the test skips only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::lock_php_worker;
use bstr::{BStr, BString};
use shirabe_php_shim::os_str_from_bytes;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-process-executor")
}

struct Run {
    exit_code: i32,
    trace: BString,
}

/// Runs `install` in a fresh copy of the fixture and returns the exit code with the plugin's trace.
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
            std::fs::read(project.join("process-executor-trace.txt")).unwrap_or_default(),
        ),
    }
}

#[test]
fn test_plugin_owned_process_executor_matches_upstream_composer() {
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
    // timeout is the project's `process-timeout`, which is what makes it evidence that both
    // worlds read one value rather than each holding its own default.
    assert_eq!(
        "\
event=post-update-cmd
timeout=42
timeout-after-set=7
capture code=0 output=\"captured\\n\" error=\"\"
list code=0 output=\"from a list\\n\"
failing code=3 output=\"out\\n\" error=\"err\\n\"
forwarded code=0 file=\"forwarded\"
callback code=0 seen=[\"out:through-a-callback\"] argument=true
reentrant code=0 nested=[\"outer code=0 output=\\\"inner\\\\n\\\" error=\\\"inner-err\\\\n\\\"\"] error=\"\"
throwing caught=RuntimeException message=\"from the callback: first\" code=7 calls=1
cwd code=0 basename=\"vendor\"
splitLines code=0 lines=[\"x\",\"y\"] empty=[]
escape=\"'a b'\\\\''c'\"
requiresGitDirEnv status=false
maxJobs=ok
filesystem normalizePath=\"\\/a\\/c\" isLocalPath=true getPlatformPath=\"\\/a\\/b\"
",
        upstream.trace
    );
}
