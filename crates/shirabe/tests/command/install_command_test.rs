//! ref: composer/tests/Composer/Test/Command/InstallCommandTest.php

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_install_command_errors);
stub!(test_install_from_empty_vendor);
stub!(test_install_from_empty_vendor_no_dev);
stub!(test_install_new_packages_with_existing_partial_vendor);
