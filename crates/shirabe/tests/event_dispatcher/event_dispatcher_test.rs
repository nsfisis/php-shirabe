//! ref: composer/tests/Composer/Test/EventDispatcher/EventDispatcherTest.php

use shirabe::util::platform::Platform;

fn tear_down() {
    Platform::clear_env("COMPOSER_SKIP_SCRIPTS");
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

// These build an EventDispatcher with a mocked Composer/IO/ProcessExecutor and run script
// listeners (executing CLI/PHP callbacks); mocking and the script-execution machinery are
// not available here.
#[test]
#[ignore = "mocks Composer/IO/ProcessExecutor and executes script listeners; not ported"]
fn test_listener_exceptions_are_caught() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "mocks Composer/IO/ProcessExecutor and executes script listeners; not ported"]
fn test_dispatcher_can_execute_single_command_line_script() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "mocks Composer/IO/ProcessExecutor and executes script listeners; not ported"]
fn test_dispatcher_pass_dev_mode_to_autoload_generator_for_script_events() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "mocks Composer/IO/ProcessExecutor and executes script listeners; not ported"]
fn test_dispatcher_remove_listener() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "mocks Composer/IO/ProcessExecutor and executes script listeners; not ported"]
fn test_dispatcher_can_execute_cli_and_php_in_same_event_script_stack() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "mocks Composer/IO/ProcessExecutor and executes script listeners; not ported"]
fn test_dispatcher_can_put_env() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "mocks Composer/IO/ProcessExecutor and executes script listeners; not ported"]
fn test_dispatcher_appends_dir_bin_on_path_for_every_listener() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "mocks Composer/IO/ProcessExecutor and executes script listeners; not ported"]
fn test_dispatcher_support_for_additional_args() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "mocks Composer/IO/ProcessExecutor and executes script listeners; not ported"]
fn test_dispatcher_can_execute_composer_script_groups() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "mocks Composer/IO/ProcessExecutor and executes script listeners; not ported"]
fn test_recursion_in_scripts_names() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "mocks Composer/IO/ProcessExecutor and executes script listeners; not ported"]
fn test_dispatcher_detect_infinite_recursion() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "mocks Composer/IO/ProcessExecutor and executes script listeners; not ported"]
fn test_dispatcher_outputs_command() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "mocks Composer/IO/ProcessExecutor and executes script listeners; not ported"]
fn test_dispatcher_outputs_error_on_failed_command() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "mocks Composer/IO/ProcessExecutor and executes script listeners; not ported"]
fn test_dispatcher_installer_events() {
    let _tear_down = TearDown;
    todo!()
}

#[test]
#[ignore = "mocks Composer/IO/ProcessExecutor and executes script listeners; not ported"]
fn test_dispatcher_doesnt_return_skipped_scripts() {
    let _tear_down = TearDown;
    todo!()
}
