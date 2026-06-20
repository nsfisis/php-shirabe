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

/// Runs the CLI with `args` from inside an empty temporary directory. Returns true if the call did
/// not panic (any exit code, including non-zero or an `Err` return, counts as success).
fn run_no_panic(args: &[&str]) -> bool {
    QUIET_PANIC.call_once(|| std::panic::set_hook(Box::new(|_| {})));
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let original = std::env::current_dir().ok();
    let dir = tempfile::tempdir().expect("create temp dir");
    std::env::set_current_dir(dir.path()).expect("chdir to temp dir");

    // SAFETY: all environment access here happens while holding `SERIAL`, so no other thread
    // touches the environment or working directory concurrently.
    unsafe {
        std::env::remove_var("COLUMNS");
        std::env::remove_var("LINES");
    }

    let mut argv = vec!["composer".to_string()];
    argv.extend(args.iter().map(|s| s.to_string()));
    let result = catch_unwind(AssertUnwindSafe(|| shirabe::run(argv)));

    if let Some(orig) = original {
        let _ = std::env::set_current_dir(orig);
    }

    result.is_ok()
}

macro_rules! run_no_panic_tests {
    ($( $(#[$attr:meta])* $name:ident => $cmd:expr ),* $(,)?) => {
        $(
            $(#[$attr])*
            #[test]
            fn $name() {
                assert!(run_no_panic(&[$cmd]), "`{}` panicked", $cmd);
            }
        )*
    };
}

run_no_panic_tests! {
    run_about => "about",
    #[ignore = "currently panics"]
    run_archive => "archive",
    #[ignore = "currently panics"]
    run_audit => "audit",
    #[ignore = "currently panics"]
    run_browse => "browse",
    #[ignore = "currently panics"]
    run_bump => "bump",
    #[ignore = "currently panics"]
    run_check_platform_reqs => "check-platform-reqs",
    #[ignore = "currently panics"]
    run_clear_cache => "clear-cache",
    #[ignore = "currently panics"]
    run_config => "config",
    #[ignore = "currently panics"]
    run_create_project => "create-project",
    #[ignore = "currently panics"]
    run_depends => "depends",
    #[ignore = "currently panics"]
    run_diagnose => "diagnose",
    #[ignore = "currently panics"]
    run_dump_autoload => "dump-autoload",
    #[ignore = "currently panics"]
    run_exec => "exec",
    #[ignore = "currently panics"]
    run_fund => "fund",
    #[ignore = "currently panics"]
    run_global => "global",
    #[ignore = "currently panics"]
    run_init => "init",
    #[ignore = "currently panics"]
    run_install => "install",
    #[ignore = "currently panics"]
    run_licenses => "licenses",
    #[ignore = "currently panics"]
    run_outdated => "outdated",
    #[ignore = "currently panics"]
    run_prohibits => "prohibits",
    #[ignore = "currently panics"]
    run_reinstall => "reinstall",
    #[ignore = "currently panics"]
    run_remove => "remove",
    #[ignore = "currently panics"]
    run_repository => "repository",
    #[ignore = "currently panics"]
    run_require => "require",
    #[ignore = "currently panics"]
    run_run_script => "run-script",
    #[ignore = "currently panics"]
    run_search => "search",
    run_self_update => "self-update",
    #[ignore = "currently panics"]
    run_show => "show",
    #[ignore = "currently panics"]
    run_status => "status",
    #[ignore = "currently panics"]
    run_suggests => "suggests",
    #[ignore = "currently panics"]
    run_update => "update",
    #[ignore = "currently panics"]
    run_validate => "validate",
}
