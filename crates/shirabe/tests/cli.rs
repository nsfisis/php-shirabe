use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Mutex, Once};

const COMMANDS: &[&str] = &[
    "about",
    "archive",
    "audit",
    "browse",
    "bump",
    "check-platform-reqs",
    "clear-cache",
    "config",
    "create-project",
    "depends",
    "diagnose",
    "dump-autoload",
    "exec",
    "fund",
    "global",
    "init",
    "install",
    "licenses",
    "outdated",
    "prohibits",
    "reinstall",
    "remove",
    "repository",
    "require",
    "run-script",
    "search",
    "self-update",
    "show",
    "status",
    "suggests",
    "update",
    "validate",
];

static QUIET_PANIC: Once = Once::new();

/// `shirabe::run` reads/writes process-global env, so concurrent invocations race;
/// serialize them since the default test harness runs tests on many threads.
static SERIAL: Mutex<()> = Mutex::new(());

/// Runs the CLI with `args`. Returns true on clean exit, false on any panic / error / non-zero
/// exit.
fn run(args: &[&str]) -> bool {
    QUIET_PANIC.call_once(|| std::panic::set_hook(Box::new(|_| {})));
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    // Each invocation must look like a fresh process.
    //
    // SAFETY: all environment access in these tests happens through `shirabe::run` while holding
    // `SERIAL`, so no other thread reads or writes the environment concurrently with these calls.
    unsafe {
        std::env::remove_var("COLUMNS");
        std::env::remove_var("LINES");
    }

    let mut argv = vec!["composer".to_string()];
    argv.extend(args.iter().map(|s| s.to_string()));
    matches!(
        catch_unwind(AssertUnwindSafe(|| shirabe::run(argv))),
        Ok(Ok(0))
    )
}

#[test]
fn version_flag() {
    assert!(run(&["--version"]));
}

#[test]
fn help_flag() {
    assert!(run(&["--help"]));
}

#[test]
fn each_command_help() {
    let failed: Vec<&&str> = COMMANDS.iter().filter(|c| !run(&[c, "--help"])).collect();
    assert!(failed.is_empty(), "`<cmd> --help` failed for: {failed:?}");
}
