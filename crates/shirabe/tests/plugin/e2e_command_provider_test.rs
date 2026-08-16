//! Plugin-provided command E2E compatibility check: upstream Composer and Shirabe each
//! install a CommandProvider fixture plugin, and then `list`, `help greet`, an actual `greet`
//! run (which itself invokes the built-in `about` command from inside the plugin command) and
//! an alias invocation are compared between the two.
//!
//! The whole fixture is Shirabe-authored (`fixtures/e2e-command/`), so nothing has to be
//! fetched; the test skips only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::{lock_php_worker, php_runtime_available};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-command")
}

struct CommandRun {
    exit_code: i32,
    stdout: String,
}

/// One composer-CLI invocation inside the prepared project.
fn run_command(work: &Path, program: &str, prefix_args: &[&str], args: &[&str]) -> CommandRun {
    let output = std::process::Command::new(program)
        .args(prefix_args)
        .args(args)
        .current_dir(work.join("project"))
        .env("COMPOSER_HOME", work.join("home"))
        .env("COMPOSER_CACHE_DIR", work.join("cache"))
        .env("COMPOSER_NO_INTERACTION", "1")
        // Rendering width must not depend on the invoking terminal.
        .env("COLUMNS", "120")
        .env("LINES", "30")
        .output()
        .unwrap();
    CommandRun {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
    }
}

fn greet_lines(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|line| line.starts_with("greet:"))
        .collect()
}

fn list_lines_mentioning_greet(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|line| line.contains("greet"))
        .collect()
}

#[test]
fn test_command_provider_execution_matches_upstream_composer() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();

    let composer_bin = composer_bin.to_str().unwrap().to_string();
    let implementations: [(&str, Vec<&str>); 2] = [
        ("php", vec![composer_bin.as_str()]),
        (env!("CARGO_BIN_EXE_shirabe"), vec![]),
    ];

    let mut results: Vec<(
        CommandRun,
        CommandRun,
        CommandRun,
        CommandRun,
        CommandRun,
        String,
    )> = Vec::new();
    for (program, prefix) in &implementations {
        let work = TempDir::new().unwrap();
        copy_dir(&fixture_dir(), work.path());
        let install = run_command(work.path(), program, prefix, &["install"]);
        assert_eq!(0, install.exit_code, "{program}: install must succeed");

        let observed = work.path().join("project/observed.txt");
        let observed_arg = format!("--out={}", observed.to_str().unwrap());
        let greet = run_command(
            work.path(),
            program,
            prefix,
            &["greet", "World", "--shout", &observed_arg],
        );
        let observed = std::fs::read_to_string(&observed).unwrap_or_default();
        let alias = run_command(
            work.path(),
            program,
            prefix,
            &["hi", "there", "--out=o2.txt"],
        );
        let help = run_command(work.path(), program, prefix, &["help", "greet"]);
        let list = run_command(work.path(), program, prefix, &["list"]);
        let missing_argument = run_command(work.path(), program, prefix, &["greet"]);
        results.push((greet, alias, help, list, missing_argument, observed));
    }

    let [upstream, shirabe] = <[_; 2]>::try_from(results).ok().unwrap();
    let (u_greet, u_alias, u_help, u_list, u_missing, u_observed) = upstream;
    let (s_greet, s_alias, s_help, s_list, s_missing, s_observed) = shirabe;

    assert_eq!(0, u_greet.exit_code, "upstream greet must succeed");
    assert_eq!(u_greet.exit_code, s_greet.exit_code);
    assert_eq!(greet_lines(&u_greet.stdout), greet_lines(&s_greet.stdout));
    assert_eq!(vec!["greet: HELLO WORLD"], greet_lines(&s_greet.stdout));

    // The file the plugin command wrote proves it observed the shared object graph — the root
    // package through requireComposer(), a strict get_class() on the application FQCN, and the
    // exit code of the built-in `about` command it invoked through the application.
    assert_eq!(u_observed, s_observed);
    assert_eq!(
        "root=shirabe/e2e-command-provider\napp=Composer\\Console\\Application\nabout=0\n",
        s_observed
    );

    assert_eq!(0, u_alias.exit_code);
    assert_eq!(u_alias.exit_code, s_alias.exit_code);
    assert_eq!(greet_lines(&u_alias.stdout), greet_lines(&s_alias.stdout));

    assert_eq!(0, u_help.exit_code, "upstream help greet must succeed");
    assert_eq!(u_help.exit_code, s_help.exit_code);
    assert_eq!(u_help.stdout, s_help.stdout, "help greet output differs");

    assert_eq!(0, u_list.exit_code);
    assert_eq!(u_list.exit_code, s_list.exit_code);
    assert_eq!(
        list_lines_mentioning_greet(&u_list.stdout),
        list_lines_mentioning_greet(&s_list.stdout)
    );

    // A validation failure (missing required argument) must fail on both sides; the rendering
    // of the error is implementation-owned and is not compared.
    assert_ne!(0, u_missing.exit_code);
    assert_ne!(0, s_missing.exit_code);
}
