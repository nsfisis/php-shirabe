//! Script-provided command E2E compatibility check: a `composer.json` script naming a
//! `Symfony\Component\Console\Command\Command` subclass is imported as an application command,
//! and `list`, `help`, executions and the mismatched-name warning are compared between upstream
//! Composer and Shirabe.
//!
//! The whole fixture is Shirabe-authored (`fixtures/e2e-script-command/`), so nothing has to be
//! fetched; the test skips only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::lock_php_worker;
use bstr::{BStr, BString, ByteSlice as _};
use shirabe_php_shim::os_str_from_bytes;

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/plugin/fixtures/e2e-script-command")
}

pub(crate) struct CommandRun {
    pub(crate) exit_code: i32,
    pub(crate) stdout: BString,
    pub(crate) stderr: BString,
}

/// One composer-CLI invocation inside the prepared project.
pub(crate) fn run_command(
    work: &std::path::Path,
    program: &BStr,
    prefix_args: &[&std::ffi::OsStr],
    args: &[&BStr],
) -> CommandRun {
    let output = std::process::Command::new(os_str_from_bytes(&program))
        .args(prefix_args)
        .args(args.iter().map(os_str_from_bytes))
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
        stdout: BString::from(output.stdout),
        stderr: BString::from(output.stderr),
    }
}

/// `list` opens with the application banner (logo and version line), which Shirabe owns and
/// upstream Composer cannot match. Everything from the `Usage:` section down still has to.
fn list_body(text: &[u8]) -> &BStr {
    let usage = text
        .find("\nUsage:")
        .expect("list output has a Usage section");
    text[usage + 1..].as_bstr()
}

#[expect(
    clippy::disallowed_methods,
    reason = "the test splits a child process's output into lines, which no PHP code does"
)]
pub(crate) fn lines_starting_with<'a>(text: &'a [u8], prefix: &BStr) -> Vec<&'a BStr> {
    text.lines()
        .map(|line| line.trim_end().as_bstr())
        .filter(|line| line.trim_start().starts_with(prefix.as_bytes()))
        .collect()
}

#[test]
fn test_script_command_class_import_matches_upstream_composer() {
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();
    let implementations: [(&BStr, Vec<&std::ffi::OsStr>); 2] = [
        (BStr::new("php"), vec![composer_bin.as_os_str()]),
        (BStr::new(env!("CARGO_BIN_EXE_shirabe")), vec![]),
    ];

    let mut results: Vec<[CommandRun; 6]> = Vec::new();
    for (program, prefix) in &implementations {
        let work = tempfile::TempDir::new().unwrap();
        copy_dir(&fixture_dir(), work.path());
        let install = run_command(work.path(), program, prefix, &[BStr::new("install")]);
        assert_eq!(
            0,
            install.exit_code,
            "{program}: install must succeed",
            program = program.display_lossy()
        );

        let greet = run_command(
            work.path(),
            program,
            prefix,
            &[BStr::new("greet"), BStr::new("World"), BStr::new("--shout")],
        );
        let renamed = run_command(work.path(), program, prefix, &[BStr::new("renamed")]);
        let help = run_command(
            work.path(),
            program,
            prefix,
            &[BStr::new("help"), BStr::new("greet")],
        );
        let list = run_command(work.path(), program, prefix, &[BStr::new("list")]);
        let thrown_exception = run_command(
            work.path(),
            program,
            prefix,
            &[BStr::new("run-script"), BStr::new("throw-exception")],
        );
        let thrown_error = run_command(
            work.path(),
            program,
            prefix,
            &[BStr::new("run-script"), BStr::new("throw-error")],
        );
        results.push([greet, renamed, help, list, thrown_exception, thrown_error]);
    }

    let [upstream, shirabe] = <[_; 2]>::try_from(results).ok().unwrap();
    let [u_greet, u_renamed, u_help, u_list, u_exception, u_error] = upstream;
    let [s_greet, s_renamed, s_help, s_list, s_exception, s_error] = shirabe;

    // The imported command runs with its own definition bound: the argument, the shorthand
    // option, the name the constructor took from composer.json and the hosting application.
    assert_eq!(0, u_greet.exit_code, "upstream greet must succeed");
    assert_eq!(u_greet.exit_code, s_greet.exit_code);
    assert_eq!(
        lines_starting_with(&u_greet.stdout, BStr::new("greet:")),
        lines_starting_with(&s_greet.stdout, BStr::new("greet:"))
    );
    assert_eq!(
        vec![
            "greet: HELLO WORLD",
            "greet: name=greet",
            "greet: app=Composer\\Console\\Application",
        ],
        lines_starting_with(&s_greet.stdout, BStr::new("greet:"))
    );

    // A class whose configure() sets a different name is renamed to the script name, and an
    // empty description is filled in from scripts-descriptions.
    assert_eq!(0, u_renamed.exit_code, "upstream renamed must succeed");
    assert_eq!(u_renamed.exit_code, s_renamed.exit_code);
    assert_eq!(
        lines_starting_with(&u_renamed.stdout, BStr::new("renamed:")),
        lines_starting_with(&s_renamed.stdout, BStr::new("renamed:"))
    );
    assert_eq!(
        vec![
            "renamed: name=renamed",
            "renamed: description=Description taken from composer.json",
        ],
        lines_starting_with(&s_renamed.stdout, BStr::new("renamed:"))
    );
    let mismatch_warning =
        "The script named renamed in composer.json has a mismatched name in its class definition.";
    assert!(
        u_renamed.stderr.contains_str(mismatch_warning),
        "upstream must warn about the mismatched name: {}",
        u_renamed.stderr.display_lossy()
    );
    assert!(
        s_renamed.stderr.contains_str(mismatch_warning),
        "shirabe must warn about the mismatched name: {}",
        s_renamed.stderr.display_lossy()
    );

    // A class extending SingleCommandApplication is still imported, with a warning.
    let single_warning = "The script named single extends SingleCommandApplication which is not compatible with Composer 2.9+";
    assert!(
        u_list.stderr.contains_str(single_warning),
        "upstream must warn about SingleCommandApplication: {}",
        u_list.stderr.display_lossy()
    );
    assert!(
        s_list.stderr.contains_str(single_warning),
        "shirabe must warn about SingleCommandApplication: {}",
        s_list.stderr.display_lossy()
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
        lines_starting_with(&s_list.stdout, BStr::new("greet"))
    );

    // Run through the event dispatcher, the class runs in a bare console application whose run
    // alone is wrapped in a catch of \Exception: an exception gets the "terminated with an
    // exception" line before it is rethrown, an \Error escapes without it.
    let terminated = "Script ShirabeTest\\ScriptCommand\\ThrowingExceptionCommand handling the throw-exception event terminated with an exception";
    assert_ne!(0, u_exception.exit_code);
    assert_eq!(u_exception.exit_code, s_exception.exit_code);
    assert!(
        u_exception.stderr.contains_str(terminated),
        "{}",
        u_exception.stderr.display_lossy()
    );
    assert!(
        s_exception.stderr.contains_str(terminated),
        "{}",
        s_exception.stderr.display_lossy()
    );
    assert!(
        s_exception.stderr.contains_str("thrown by the command"),
        "{}",
        s_exception.stderr.display_lossy()
    );
    assert_eq!(
        lines_starting_with(&u_exception.stdout, BStr::new("throw-exception:")),
        lines_starting_with(&s_exception.stdout, BStr::new("throw-exception:"))
    );

    assert_ne!(0, u_error.exit_code);
    assert_eq!(u_error.exit_code, s_error.exit_code);
    assert!(
        !u_error.stderr.contains_str("terminated with an exception"),
        "{}",
        u_error.stderr.display_lossy()
    );
    assert!(
        !s_error.stderr.contains_str("terminated with an exception"),
        "{}",
        s_error.stderr.display_lossy()
    );
    assert_eq!(
        lines_starting_with(&u_error.stdout, BStr::new("throw-error:")),
        lines_starting_with(&s_error.stdout, BStr::new("throw-error:"))
    );
}
