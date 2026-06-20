//! ref: composer/tests/Composer/Test/Command/BaseDependencyCommandTest.php

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_exception_when_no_required_parameters);
stub!(test_exception_when_running_locked_without_lock_file);
stub!(test_exception_when_it_could_not_found_the_package);
stub!(test_exception_when_package_was_not_found_in_project);
stub!(test_warning_when_dependencies_are_not_installed);
stub!(test_why_command_outputs);
stub!(test_why_not_command_outputs);
