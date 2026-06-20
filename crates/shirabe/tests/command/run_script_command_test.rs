//! ref: composer/tests/Composer/Test/Command/RunScriptCommandTest.php

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_detect_and_pass_dev_mode_to_event_and_to_dispatching);
stub!(test_can_list_scripts);
stub!(test_can_define_aliases);
stub!(test_execution_of_simple_symfony_command);
stub!(test_execution_of_symfony_command_with_configuration);
