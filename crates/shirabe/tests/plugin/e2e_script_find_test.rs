//! Script-provided command lookup E2E compatibility check: a `composer.json` script naming a
//! `Symfony\Component\Console\Command\Command` subclass `find()`s and runs, through its
//! application, a plain shell-command script listed after it, compared between upstream
//! Composer and Shirabe.
//!
//! The whole fixture is Shirabe-authored (`fixtures/e2e-script-find/`), so nothing has to be
//! fetched; the test skips only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::e2e_script_command_test::{CommandRun, lines_starting_with, run_command};
use crate::php_worker::{lock_php_worker, php_runtime_available};
use bstr::BStr;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-script-find")
}

#[test]
fn test_script_command_finds_later_script_alias_command_like_upstream_composer() {
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

    let mut results: Vec<CommandRun> = Vec::new();
    for (program, prefix) in &implementations {
        let work = TempDir::new().unwrap();
        copy_dir(&fixture_dir(), work.path());
        let install = run_command(work.path(), program, prefix, &[BStr::new("install")]);
        assert_eq!(
            0,
            install.exit_code,
            "{program}: install must succeed",
            program = program.display_lossy()
        );

        results.push(run_command(
            work.path(),
            program,
            prefix,
            &[BStr::new("finder")],
        ));
    }

    let [upstream, shirabe] = <[_; 2]>::try_from(results).ok().unwrap();

    // The plain script is registered after the Command class script, yet the class's
    // application still knows it, and running it executes the shell command.
    assert_eq!(
        0,
        upstream.exit_code,
        "upstream finder must succeed: {}",
        upstream.stderr.display_lossy()
    );
    assert_eq!(
        upstream.exit_code,
        shirabe.exit_code,
        "shirabe: {}",
        shirabe.stderr.display_lossy()
    );
    assert_eq!(
        lines_starting_with(&upstream.stdout, BStr::new("finder:")),
        lines_starting_with(&shirabe.stdout, BStr::new("finder:"))
    );
    assert_eq!(
        vec![
            "finder: has=true",
            "finder: name=plain",
            "finder: description=Runs the plain script as defined in composer.json",
            "finder: exit=0",
        ],
        lines_starting_with(&shirabe.stdout, BStr::new("finder:"))
    );
    assert_eq!(
        lines_starting_with(&upstream.stdout, BStr::new("plain-script")),
        lines_starting_with(&shirabe.stdout, BStr::new("plain-script"))
    );
    assert_eq!(
        vec!["plain-script"],
        lines_starting_with(&shirabe.stdout, BStr::new("plain-script"))
    );
}
