//! ref: composer/tests/Composer/Test/ApplicationTest.php

// These drive the console Application (doRun, getDisplay, plugin disabling, command
// resolution) via ApplicationTester, none of which are ported.

use shirabe::util::platform::Platform;

fn set_up() {
    Platform::put_env("COMPOSER_DISABLE_XDEBUG_WARN", "1");
}

fn tear_down() {
    Platform::clear_env("COMPOSER_DISABLE_XDEBUG_WARN");
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

#[test]
#[ignore = "requires the console Application/ApplicationTester harness, which is not yet ported"]
fn test_dev_warning() {
    let _tear_down = TearDown;
    set_up();

    todo!()
}

#[test]
#[ignore = "requires the console Application/ApplicationTester harness, which is not yet ported"]
fn test_dev_warning_suppressed_for_self_update() {
    let _tear_down = TearDown;
    set_up();

    todo!()
}

#[test]
#[ignore = "requires the console Application/ApplicationTester harness, which is not yet ported"]
fn test_process_isolation_works_multiple_times() {
    let _tear_down = TearDown;
    set_up();

    todo!()
}

#[test]
#[ignore = "requires the console Application/ApplicationTester harness, which is not yet ported"]
fn test_no_plugins_disables_plugins_when_script_commands_exist() {
    let _tear_down = TearDown;
    set_up();

    todo!()
}

#[test]
#[ignore = "requires the console Application/ApplicationTester harness, which is not yet ported"]
fn test_script_command_takes_priority_over_abbreviated_builtin_command() {
    let _tear_down = TearDown;
    set_up();

    todo!()
}
