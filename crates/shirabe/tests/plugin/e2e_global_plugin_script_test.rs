//! Script class autoload E2E compatibility check: upstream Composer and Shirabe each run `install`
//! in a fixture project whose `post-autoload-dump` script calls a class of a globally installed
//! plugin the project does not require, appending what they observe to a trace file. Only the
//! global plugin's class loader can resolve that class. The global installation is prepared
//! with upstream Composer for both runs. Upstream has no such test, so the whole fixture is
//! Shirabe-authored (`fixtures/e2e-global-plugin-script/`) and nothing has to be fetched; the
//! test skips only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::{lock_php_worker, php_runtime_available};
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::os_str_from_bytes;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-global-plugin-script")
}

struct Run {
    exit_code: i32,
    trace: BString,
}

fn command(program: &BStr, prefix_args: &[&OsStr], work: &Path) -> std::process::Command {
    let mut command = std::process::Command::new(os_str_from_bytes(program));
    command
        .args(prefix_args)
        .env("COMPOSER_HOME", work.join("home"))
        .env("COMPOSER_CACHE_DIR", work.join("cache"))
        .env("COMPOSER_NO_INTERACTION", "1")
        .env("COLUMNS", "120")
        .env("LINES", "30");
    command
}

/// Installs the global plugin with upstream Composer, runs `install` in the project with
/// `program`, and returns the exit code with the trace.
fn install(composer_bin: &Path, program: &BStr, prefix_args: &[&OsStr]) -> Run {
    let work = TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());

    let global = command(BStr::new("php"), &[composer_bin.as_os_str()], work.path())
        .args(["global", "install"])
        .output()
        .unwrap();
    assert!(
        global.status.success(),
        "upstream global install must succeed: {}",
        global.stderr.as_bstr().display_lossy()
    );

    let project = work.path().join("project");
    let output = command(program, prefix_args, work.path())
        .arg("install")
        .current_dir(&project)
        .output()
        .unwrap();
    Run {
        exit_code: output.status.code().unwrap_or(-1),
        trace: BString::from(
            std::fs::read(project.join("global-plugin-script-trace.txt")).unwrap_or_default(),
        ),
    }
}

#[test]
fn test_script_calls_a_class_of_a_global_plugin() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();

    let upstream = install(&composer_bin, BStr::new("php"), &[composer_bin.as_os_str()]);
    let shirabe = install(&composer_bin, BStr::new(env!("CARGO_BIN_EXE_shirabe")), &[]);

    assert_eq!(0, upstream.exit_code, "upstream install must succeed");
    assert_eq!(upstream.exit_code, shirabe.exit_code);
    assert_eq!(upstream.trace, shirabe.trace);

    // Pinned as well as compared, so a run where neither side wrote a trace cannot pass.
    assert_eq!(
        "\
activate handler-loaded=false
script post-autoload-dump
",
        upstream.trace
    );
}
