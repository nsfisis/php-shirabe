//! Script-provided command E2E compatibility check: a `composer.json` script naming a
//! `Symfony\Component\Console\Command\Command` subclass is imported as an application command,
//! and `list`, `help`, executions and the mismatched-name warning are compared between upstream
//! Composer and Shirabe.
//!
//! The whole fixture is Shirabe-authored (`fixtures/e2e-script-command/`), so nothing has to be
//! fetched; the test skips only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::plugin_installer_test::{lock_php_worker, php_runtime_available};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-script-command")
}

struct CommandRun {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

/// One composer-CLI invocation inside the prepared project.
fn run_command(work: &Path, program: &str, prefix_args: &[&str], args: &[&str]) -> CommandRun {
    let output = std::process::Command::new(program)
        .args(prefix_args)
        .args(args)
        .current_dir(work)
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
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// `list` opens with the application banner (logo and version line), which Shirabe owns and
/// upstream Composer cannot match. Everything from the `Usage:` section down still has to.
fn list_body(text: &str) -> &str {
    let usage = text
        .find("\nUsage:")
        .expect("list output has a Usage section");
    &text[usage + 1..]
}

fn lines_starting_with<'a>(text: &'a str, prefix: &str) -> Vec<&'a str> {
    text.lines()
        .map(str::trim_end)
        .filter(|line| line.trim_start().starts_with(prefix))
        .collect()
}

#[test]
fn test_script_command_class_import_matches_upstream_composer() {
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

    let mut results: Vec<[CommandRun; 4]> = Vec::new();
    for (program, prefix) in &implementations {
        let work = TempDir::new().unwrap();
        copy_dir(&fixture_dir(), work.path());
        let install = run_command(work.path(), program, prefix, &["install"]);
        assert_eq!(0, install.exit_code, "{program}: install must succeed");

        let greet = run_command(work.path(), program, prefix, &["greet", "World", "--shout"]);
        let renamed = run_command(work.path(), program, prefix, &["renamed"]);
        let help = run_command(work.path(), program, prefix, &["help", "greet"]);
        let list = run_command(work.path(), program, prefix, &["list"]);
        results.push([greet, renamed, help, list]);
    }

    let [upstream, shirabe] = <[_; 2]>::try_from(results).ok().unwrap();
    let [u_greet, u_renamed, u_help, u_list] = upstream;
    let [s_greet, s_renamed, s_help, s_list] = shirabe;

    // The imported command runs with its own definition bound: the argument, the shorthand
    // option, the name the constructor took from composer.json and the hosting application.
    assert_eq!(0, u_greet.exit_code, "upstream greet must succeed");
    assert_eq!(u_greet.exit_code, s_greet.exit_code);
    assert_eq!(
        lines_starting_with(&u_greet.stdout, "greet:"),
        lines_starting_with(&s_greet.stdout, "greet:")
    );
    assert_eq!(
        vec![
            "greet: HELLO WORLD",
            "greet: name=greet",
            "greet: app=Composer\\Console\\Application",
        ],
        lines_starting_with(&s_greet.stdout, "greet:")
    );

    // A class whose configure() sets a different name is renamed to the script name, and an
    // empty description is filled in from scripts-descriptions.
    assert_eq!(0, u_renamed.exit_code, "upstream renamed must succeed");
    assert_eq!(u_renamed.exit_code, s_renamed.exit_code);
    assert_eq!(
        lines_starting_with(&u_renamed.stdout, "renamed:"),
        lines_starting_with(&s_renamed.stdout, "renamed:")
    );
    assert_eq!(
        vec![
            "renamed: name=renamed",
            "renamed: description=Description taken from composer.json",
        ],
        lines_starting_with(&s_renamed.stdout, "renamed:")
    );
    let mismatch_warning =
        "The script named renamed in composer.json has a mismatched name in its class definition.";
    assert!(
        u_renamed.stderr.contains(mismatch_warning),
        "upstream must warn about the mismatched name: {}",
        u_renamed.stderr
    );
    assert!(
        s_renamed.stderr.contains(mismatch_warning),
        "shirabe must warn about the mismatched name: {}",
        s_renamed.stderr
    );

    // A class extending SingleCommandApplication is still imported, with a warning.
    let single_warning = "The script named single extends SingleCommandApplication which is not compatible with Composer 2.9+";
    assert!(
        u_list.stderr.contains(single_warning),
        "upstream must warn about SingleCommandApplication: {}",
        u_list.stderr
    );
    assert!(
        s_list.stderr.contains(single_warning),
        "shirabe must warn about SingleCommandApplication: {}",
        s_list.stderr
    );

    assert_eq!(0, u_help.exit_code, "upstream help greet must succeed");
    assert_eq!(u_help.exit_code, s_help.exit_code);
    assert_eq!(u_help.stdout, s_help.stdout, "help greet output differs");

    // The imported commands are listed with the descriptions the class and composer.json give
    // them, next to the plain shell script that stays a ScriptAliasCommand.
    assert_eq!(0, u_list.exit_code);
    assert_eq!(u_list.exit_code, s_list.exit_code);
    assert_eq!(
        list_body(&u_list.stdout),
        list_body(&s_list.stdout),
        "list output differs"
    );
    assert_eq!(
        vec!["  greet                Greets someone from a script-provided command."],
        lines_starting_with(&s_list.stdout, "greet")
    );
}
