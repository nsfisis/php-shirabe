//! InstalledVersions E2E compatibility check: upstream Composer and Shirabe each run a PHP script
//! in a fixture project whose `vendor/composer/installed.php` already exists, and the script
//! records what `Composer\InstalledVersions` reports. Nothing rewrites `installed.php` during the
//! run, so what the script sees is the state Composer loads while creating the Composer instance.
//! Upstream has no such test, so the whole fixture is Shirabe-authored
//! (`fixtures/e2e-installed-versions/`) and nothing has to be fetched; the test skips only while
//! the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::lock_php_worker;
use bstr::{BStr, BString};
use shirabe_php_shim::{bformat, os_str_from_bytes};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-installed-versions")
}

struct Run {
    exit_code: i32,
    output: BString,
    trace: BString,
}

/// Runs the `probe` script in a fresh copy of the fixture and returns the exit code with what the
/// script wrote to its trace file.
fn run_probe(program: &BStr, prefix_args: &[&OsStr]) -> Run {
    let work = TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let project = work.path().join("project");
    let output = std::process::Command::new(os_str_from_bytes(program))
        .args(prefix_args)
        .args(["run-script", "probe"])
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
        output: bformat!("{}{}", &output.stdout, &output.stderr),
        trace: BString::from(
            std::fs::read(project.join("installed-versions-trace.txt")).unwrap_or_default(),
        ),
    }
}

#[test]
fn test_installed_versions_is_loaded_from_the_project_before_scripts_run() {
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();

    let upstream = run_probe(BStr::new("php"), &[composer_bin.as_os_str()]);
    let shirabe = run_probe(BStr::new(env!("CARGO_BIN_EXE_shirabe")), &[]);

    assert_eq!(
        0,
        upstream.exit_code,
        "upstream run-script must succeed: {}",
        upstream.output.display_lossy()
    );
    assert_eq!(
        upstream.exit_code,
        shirabe.exit_code,
        "shirabe output: {}",
        shirabe.output.display_lossy()
    );
    assert_eq!(upstream.trace, shirabe.trace);

    // Pinned as well as compared, so a run where neither side loads the project's installed.php
    // cannot pass.
    assert_eq!(
        "\
installed=1
pretty-version=2.3.4
install-path=/vendor/composer/../shirabe/seeded
",
        upstream.trace
    );
}
