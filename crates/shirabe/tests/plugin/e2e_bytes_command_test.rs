//! Non-UTF-8 command metadata E2E compatibility check: a command declaring a description, aliases,
//! help, a usage and option and argument descriptions that are not valid UTF-8 is listed, described
//! and run, and the bytes are compared between upstream Composer and Shirabe.
//!
//! The whole fixture is Shirabe-authored (`fixtures/e2e-bytes-command/`), so nothing has to be
//! fetched; the test skips only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::{lock_php_worker, php_runtime_available};
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::os_str_from_bytes;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-bytes-command")
}

/// One composer-CLI invocation inside the prepared project, returning its exit code and stdout.
fn run_command(
    work: &Path,
    program: &BStr,
    prefix_args: &[&OsStr],
    args: &[&[u8]],
) -> (i32, BString) {
    let output = std::process::Command::new(os_str_from_bytes(&program))
        .args(prefix_args)
        .args(args.iter().map(|arg| OsStr::from_bytes(arg)))
        .current_dir(work)
        .env("COMPOSER_HOME", work.join("home"))
        .env("COMPOSER_CACHE_DIR", work.join("cache"))
        .env("COMPOSER_NO_INTERACTION", "1")
        .env("COLUMNS", "120")
        .env("LINES", "30")
        .output()
        .unwrap();
    (
        output.status.code().unwrap_or(-1),
        BString::from(output.stdout),
    )
}

/// The lines naming the fixture's command. The rest of a listing holds the application name and
/// the built-in commands' help, which Shirabe words differently.
#[expect(
    clippy::disallowed_methods,
    reason = "the test splits a child process's output into lines, which no PHP code does"
)]
fn fixture_lines(text: &[u8]) -> Vec<&BStr> {
    text.lines()
        .map(|line| line.as_bstr())
        .filter(|line| line.contains(&0xE9) || line.contains_str("bytes"))
        .collect()
}

#[test]
fn test_non_utf8_command_metadata_matches_upstream_composer() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();
    let implementations: [(&BStr, Vec<&OsStr>); 2] = [
        (BStr::new("php"), vec![composer_bin.as_os_str()]),
        (BStr::new(env!("CARGO_BIN_EXE_shirabe")), vec![]),
    ];
    let cases: [&[&[u8]]; 13] = [
        &[b"list"],
        &[b"list", b"--raw"],
        &[b"list", b"--format=md"],
        &[b"list", b"--format=xml"],
        &[b"list", b"namespace\xE9"],
        &[b"help", b"bytes"],
        &[b"help", b"bytes", b"--format=md"],
        &[b"help", b"bytes", b"--format=xml"],
        &[b"bytes"],
        &[b"alias\xE9"],
        &[b"namespace\xE9:sub"],
        &[b"namespace\xE9:su"],
        &[b"run-script", b"undefined\xE9"],
    ];

    let mut results: Vec<Vec<(i32, BString)>> = Vec::new();
    for (program, prefix) in &implementations {
        let work = TempDir::new().unwrap();
        copy_dir(&fixture_dir(), work.path());
        let (install, _) = run_command(work.path(), program, prefix, &[b"install"]);
        assert_eq!(
            0,
            install,
            "{program}: install must succeed",
            program = program.display_lossy()
        );
        results.push(
            cases
                .iter()
                .map(|args| run_command(work.path(), program, prefix, args))
                .collect(),
        );
    }

    for (index, args) in cases.iter().enumerate() {
        let (upstream_code, upstream_stdout) = &results[0][index];
        let (shirabe_code, shirabe_stdout) = &results[1][index];
        assert_eq!(upstream_code, shirabe_code, "exit code of {args:?}");
        assert_eq!(
            fixture_lines(upstream_stdout),
            fixture_lines(shirabe_stdout),
            "stdout of {args:?}"
        );
    }

    // The comparison above would also pass if neither side printed the command at all.
    let (_, help) = &results[1][5];
    assert!(help.contains_str(b"description-\xE9"));
    assert!(help.contains_str(b"alias\xE9"));
    assert!(help.contains_str(b"usage-\xE9"));
    assert!(help.contains_str(b"help-\xE9"));
    assert!(help.contains_str(b"argument-description-\xE9"));
    assert!(help.contains_str(b"option-description-\xE9"));
    for index in [8, 9, 10, 11] {
        assert!(results[1][index].1.contains_str("bytes: ran"));
    }
}
