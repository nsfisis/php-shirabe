//! Exception fidelity E2E check: upstream Composer and Shirabe each install a fixture project
//! whose plugin catches the exceptions a Composer service raises at it and writes what it saw to
//! a trace file. Upstream has no test that inspects an exception from plugin code, so the whole
//! fixture is Shirabe-authored (`fixtures/e2e-exception/`) and nothing has to be fetched; the
//! test skips only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::{lock_php_worker, php_runtime_available};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-exception")
}

struct Run {
    exit_code: i32,
    trace: String,
}

/// Runs `install` in a fresh copy of the fixture and returns the exit code with the plugin's trace.
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
    Run {
        exit_code: output.status.code().unwrap_or(-1),
        trace: std::fs::read_to_string(project.join("exception-trace.txt")).unwrap_or_default(),
    }
}

#[test]
fn test_exceptions_reach_a_plugin_as_their_own_class() {
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

    // Pinned as well as compared, so a run where neither side wrote a trace cannot pass. The
    // class names and the two hierarchy answers are the evidence that the exception crossed as
    // itself rather than as one collapsed shape.
    assert_eq!(
        "\
event=post-update-cmd
findShortestPath class=\"InvalidArgumentException\" \
message=\"$from (relative) and $to (\\/absolute) must be absolute paths.\" code=0 \
logic=true runtime=false
findShortestPathCode class=\"InvalidArgumentException\" \
message=\"$from (\\/absolute) and $to (relative) must be absolute paths.\" code=0 \
logic=true runtime=false
ensureDirectoryExists class=\"RuntimeException\" \
message=\"not-a-directory exists and is not a directory.\" code=0 logic=false runtime=true
catch-clause=InvalidArgumentException
",
        upstream.trace
    );
}
