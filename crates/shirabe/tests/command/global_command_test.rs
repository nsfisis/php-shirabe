//! ref: composer/tests/Composer/Test/Command/GlobalCommandTest.php

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_global);
stub!(test_cannot_create_home);
stub!(test_global_show);
stub!(test_global_show_without_packages);
stub!(test_global_require);
stub!(test_global_update);
stub!(test_global_changes_directory);
stub!(test_global_missing_command_name);
