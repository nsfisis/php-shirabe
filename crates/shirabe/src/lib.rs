pub mod advisory;
pub mod autoload;
pub mod cache;
pub mod command;
pub mod composer;
pub mod config;
pub mod console;
pub mod dependency_resolver;
pub mod downloader;
pub mod event_dispatcher;
pub mod exception;
pub mod factory;
pub mod filter;
pub mod installed_versions;
pub mod installer;
pub mod io;
pub mod json;
pub mod package;
pub mod phpstan;
pub mod platform;
pub mod plugin;
pub mod question;
pub mod repository;
pub mod script;
pub mod self_update;
pub mod util;

pub fn run(argv: Vec<String>) -> anyhow::Result<i32> {
    use crate::console::Application;
    use crate::util::Platform;
    use shirabe_external_packages::symfony::console::input::argv_input::ArgvInput;
    use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;

    // TODO(php-runtime): the full initialization process in composer/bin/composer should be ported
    // somewhere else that communicates with the real PHP runtime.
    Platform::put_env(
        "COMPOSER_BINARY",
        &shirabe_php_shim::realpath(argv.first().map(String::as_str).unwrap_or_default())
            .unwrap_or_default(),
    );

    let application = Application::new_shared("Composer".to_string(), String::new())?;
    let input = std::rc::Rc::new(std::cell::RefCell::new(ArgvInput::new(Some(argv), None)?))
        as std::rc::Rc<std::cell::RefCell<dyn InputInterface>>;
    Application::run(&application, Some(input), None)
}

#[cfg(test)]
mod cli_tests {
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

    /// `crate::run` reads/writes process-global env, so concurrent invocations race;
    /// serialize them since the default test harness runs tests on many threads.
    static SERIAL: Mutex<()> = Mutex::new(());

    /// Runs the CLI with `args`. Returns true on clean exit, false on any panic / error / non-zero
    /// exit.
    fn run(args: &[&str]) -> bool {
        QUIET_PANIC.call_once(|| std::panic::set_hook(Box::new(|_| {})));
        let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        // Each invocation must look like a fresh process.
        //
        // SAFETY: all environment access in these tests happens through `crate::run` while holding
        // `SERIAL`, so no other thread reads or writes the environment concurrently with these calls.
        unsafe {
            std::env::remove_var("COLUMNS");
            std::env::remove_var("LINES");
        }

        let mut argv = vec!["composer".to_string()];
        argv.extend(args.iter().map(|s| s.to_string()));
        matches!(
            catch_unwind(AssertUnwindSafe(|| crate::run(argv))),
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
        let result = catch_unwind(AssertUnwindSafe(|| crate::run(argv)));

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
        run_audit => "audit",
        #[ignore = "currently panics"]
        run_browse => "browse",
        run_bump => "bump",
        run_check_platform_reqs => "check-platform-reqs",
        #[ignore = "currently panics"]
        run_clear_cache => "clear-cache",
        run_config => "config",
        run_create_project => "create-project",
        run_depends => "depends",
        #[ignore = "currently panics"]
        run_diagnose => "diagnose",
        run_dump_autoload => "dump-autoload",
        run_exec => "exec",
        run_fund => "fund",
        run_global => "global",
        #[ignore = "currently panics"]
        run_init => "init",
        run_install => "install",
        run_licenses => "licenses",
        run_outdated => "outdated",
        run_prohibits => "prohibits",
        run_reinstall => "reinstall",
        run_remove => "remove",
        run_repository => "repository",
        #[ignore = "currently panics"]
        run_require => "require",
        run_run_script => "run-script",
        run_search => "search",
        run_self_update => "self-update",
        run_show => "show",
        run_status => "status",
        run_suggests => "suggests",
        run_update => "update",
        run_validate => "validate",
    }
}
