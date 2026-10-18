//! Command event E2E compatibility check: upstream Composer and Shirabe each install a fixture
//! project and then run a few more commands, with a plugin subscribed to
//! `PluginEvents::PRE_COMMAND_RUN` and `PluginEvents::COMMAND` appending what each event exposes
//! to trace files. Upstream has no test that dispatches these events through a real plugin, so
//! the whole fixture is Shirabe-authored (`fixtures/e2e-command-event/`) and nothing has to be
//! fetched; the tests skip only while the PHP runtime or the Composer checkout is missing.

use crate::e2e_extension_installer_test::{copy_dir, upstream_composer_bin};
use crate::php_worker::{lock_php_worker, php_runtime_available};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/plugin/fixtures/e2e-command-event")
}

struct Run {
    exit_codes: Vec<i32>,
    trace: String,
    console_trace: String,
}

/// Runs `install` and then the commands whose events the plugin records in a fresh copy of the
/// fixture: the plugin is only active once the first command has installed it.
fn run_commands(program: &str, prefix_args: &[&str]) -> Run {
    let work = TempDir::new().unwrap();
    copy_dir(&fixture_dir(), work.path());
    let project = work.path().join("project");
    let commands: [&[&str]; 5] = [
        &["install"],
        &["install"],
        &["depends", "shirabe-test/lib-a"],
        &["prohibits", "shirabe-test/lib-a", "2.0"],
        &["validate"],
    ];
    let exit_codes = commands
        .iter()
        .map(|args| {
            std::process::Command::new(program)
                .args(prefix_args)
                .args(*args)
                .current_dir(&project)
                .env("COMPOSER_HOME", work.path().join("home"))
                .env("COMPOSER_CACHE_DIR", work.path().join("cache"))
                .env("COMPOSER_NO_INTERACTION", "1")
                .env("COLUMNS", "120")
                .env("LINES", "30")
                .output()
                .unwrap()
                .status
                .code()
                .unwrap_or(-1)
        })
        .collect();
    let read = |name: &str| std::fs::read_to_string(project.join(name)).unwrap_or_default();
    Run {
        exit_codes,
        trace: read("command-event-trace.txt"),
        console_trace: read("command-event-console-trace.txt"),
    }
}

#[test]
fn test_command_events_match_upstream_composer() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();
    let composer_bin = composer_bin.to_str().unwrap().to_string();

    let upstream = run_commands("php", &[composer_bin.as_str()]);
    let shirabe = run_commands(env!("CARGO_BIN_EXE_shirabe"), &[]);

    // `prohibits` finds nothing prohibiting the version and reports that with exit code 1.
    assert_eq!(vec![0, 0, 0, 1, 0], upstream.exit_codes);
    assert_eq!(upstream.exit_codes, shirabe.exit_codes);
    assert_eq!(upstream.trace, shirabe.trace);

    // Pinned as well as compared, so a run where neither side dispatches anything cannot pass.
    assert_eq!(
        "\
pre-command-run class=Composer\\Plugin\\PreCommandRunEvent command=install
command class=Composer\\Plugin\\CommandEvent commandName=install
pre-command-run class=Composer\\Plugin\\PreCommandRunEvent command=depends
command class=Composer\\Plugin\\CommandEvent commandName=depends
pre-command-run class=Composer\\Plugin\\PreCommandRunEvent command=prohibits
command class=Composer\\Plugin\\CommandEvent commandName=prohibits
pre-command-run class=Composer\\Plugin\\PreCommandRunEvent command=validate
command class=Composer\\Plugin\\CommandEvent commandName=validate
",
        upstream.trace
    );
}

#[ignore = "the console input and output of a command event do not cross into the plugin process, so getInput() and getOutput() raise an explicit error (TODO(plugin) symfony/console two-world)"]
#[test]
fn test_command_event_console_matches_upstream_composer() {
    if !php_runtime_available() {
        return;
    }
    let Some(composer_bin) = upstream_composer_bin() else {
        return;
    };
    let _worker = lock_php_worker();
    let composer_bin = composer_bin.to_str().unwrap().to_string();

    let upstream = run_commands("php", &[composer_bin.as_str()]);
    let shirabe = run_commands(env!("CARGO_BIN_EXE_shirabe"), &[]);

    assert_eq!(upstream.console_trace, shirabe.console_trace);
}
