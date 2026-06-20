//! ref: composer/tests/Composer/Test/EventDispatcher/EventDispatcherTest.php

// These build an EventDispatcher with a mocked Composer/IO/ProcessExecutor and run script
// listeners (executing CLI/PHP callbacks); mocking and the script-execution machinery are
// not available here.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks Composer/IO/ProcessExecutor and executes script listeners; not ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_listener_exceptions_are_caught);
stub!(test_dispatcher_can_execute_single_command_line_script);
stub!(test_dispatcher_pass_dev_mode_to_autoload_generator_for_script_events);
stub!(test_dispatcher_remove_listener);
stub!(test_dispatcher_can_execute_cli_and_php_in_same_event_script_stack);
stub!(test_dispatcher_can_put_env);
stub!(test_dispatcher_appends_dir_bin_on_path_for_every_listener);
stub!(test_dispatcher_support_for_additional_args);
stub!(test_dispatcher_can_execute_composer_script_groups);
stub!(test_recursion_in_scripts_names);
stub!(test_dispatcher_detect_infinite_recursion);
stub!(test_dispatcher_outputs_command);
stub!(test_dispatcher_outputs_error_on_failed_command);
stub!(test_dispatcher_installer_events);
stub!(test_dispatcher_doesnt_return_skipped_scripts);
