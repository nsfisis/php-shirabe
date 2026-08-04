//! ergebnis/composer-normalize E2E compatibility check: upstream Composer and Shirabe each
//! install the pinned plugin (with its real dependency tree) and the `list`/`help` renderings
//! of its command are compared; the execution comparison is present but ignored until the
//! worker can construct a second native Composer instance.
//!
//! Prerequisites: the PHP runtime, the Composer checkout, and the pinned packages in
//! `fixtures/e2e-normalize/ext/` — run `fixtures/e2e-normalize/fetch` once to populate it.
//! The test skips while any of these is missing; test runs themselves are offline.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::plugin_installer_test::{lock_php_worker, php_runtime_available};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-normalize")
}

fn fixture_fetched() -> bool {
    fixture_dir()
        .join("ext/composer-normalize-2.52.0/src/NormalizePlugin.php")
        .is_file()
}

struct CommandRun {
    exit_code: i32,
    stdout: String,
}

fn run_command(work: &Path, program: &str, prefix_args: &[&str], args: &[&str]) -> CommandRun {
    let output = std::process::Command::new(program)
        .args(prefix_args)
        .args(args)
        .current_dir(work.join("project"))
        .env("COMPOSER_HOME", work.join("home"))
        .env("COMPOSER_CACHE_DIR", work.join("cache"))
        .env("COMPOSER_NO_INTERACTION", "1")
        .env("COLUMNS", "120")
        .env("LINES", "30")
        .output()
        .unwrap();
    CommandRun {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
    }
}

fn prepared_project(program: &str, prefix: &[&str]) -> TempDir {
    let work = TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let install = run_command(work.path(), program, prefix, &["install"]);
    assert_eq!(0, install.exit_code, "{program}: install must succeed");
    work
}

fn normalize_lines(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|line| line.contains("normalize"))
        .collect()
}

#[test]
fn test_normalize_listing_matches_upstream_composer() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    if !fixture_fetched() {
        return;
    }
    let _worker = lock_php_worker();

    let composer_bin = composer_bin.to_str().unwrap().to_string();
    let upstream_prefix = [composer_bin.as_str()];
    let u_work = prepared_project("php", &upstream_prefix);
    let s_work = prepared_project(env!("CARGO_BIN_EXE_shirabe"), &[]);

    let u_list = run_command(
        u_work.path(),
        "php",
        &upstream_prefix,
        &["list", "--no-ansi"],
    );
    let s_list = run_command(
        s_work.path(),
        env!("CARGO_BIN_EXE_shirabe"),
        &[],
        &["list", "--no-ansi"],
    );
    assert_eq!(0, u_list.exit_code);
    assert_eq!(u_list.exit_code, s_list.exit_code);
    assert_eq!(
        normalize_lines(&u_list.stdout),
        normalize_lines(&s_list.stdout)
    );
    assert!(
        !normalize_lines(&s_list.stdout).is_empty(),
        "list must mention the plugin-provided normalize command"
    );

    let u_help = run_command(
        u_work.path(),
        "php",
        &upstream_prefix,
        &["help", "normalize", "--no-ansi"],
    );
    let s_help = run_command(
        s_work.path(),
        env!("CARGO_BIN_EXE_shirabe"),
        &[],
        &["help", "normalize", "--no-ansi"],
    );
    assert_eq!(0, u_help.exit_code);
    assert_eq!(u_help.exit_code, s_help.exit_code);
    assert_eq!(
        u_help.stdout, s_help.stdout,
        "help normalize output differs"
    );
}

// TODO(plugin): NormalizeCommand::execute builds a second, in-process Composer instance
// (`(new Factory())->createComposer(...)`), and the worker's proxy stub classes reject native
// construction of the FQCNs that path instantiates (Composer\Composer, the event dispatcher,
// the package graph, ...); running the command therefore stops at that explicit error.
#[ignore = "the worker cannot construct a second native Composer instance yet; see the TODO(plugin) above"]
#[test]
fn test_normalize_execution_matches_upstream_composer() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    if !fixture_fetched() {
        return;
    }
    let _worker = lock_php_worker();

    let composer_bin = composer_bin.to_str().unwrap().to_string();
    let upstream_prefix = [composer_bin.as_str()];
    let u_work = prepared_project("php", &upstream_prefix);
    let s_work = prepared_project(env!("CARGO_BIN_EXE_shirabe"), &[]);

    let dry = ["normalize", "--dry-run", "--no-ansi"];
    let u_dry = run_command(u_work.path(), "php", &upstream_prefix, &dry);
    let s_dry = run_command(s_work.path(), env!("CARGO_BIN_EXE_shirabe"), &[], &dry);
    assert_eq!(1, u_dry.exit_code, "upstream dry-run must report a diff");
    assert_eq!(u_dry.exit_code, s_dry.exit_code);
    assert_eq!(
        u_dry.stdout, s_dry.stdout,
        "normalize --dry-run output differs"
    );

    let real = ["normalize", "--no-update-lock", "--no-ansi"];
    let u_run = run_command(u_work.path(), "php", &upstream_prefix, &real);
    let s_run = run_command(s_work.path(), env!("CARGO_BIN_EXE_shirabe"), &[], &real);
    assert_eq!(0, u_run.exit_code, "upstream normalize must succeed");
    assert_eq!(u_run.exit_code, s_run.exit_code);
    assert_eq!(
        std::fs::read(u_work.path().join("project/composer.json")).unwrap(),
        std::fs::read(s_work.path().join("project/composer.json")).unwrap(),
        "normalized composer.json differs"
    );
}
