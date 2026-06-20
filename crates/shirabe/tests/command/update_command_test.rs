//! ref: composer/tests/Composer/Test/Command/UpdateCommandTest.php

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_update);
stub!(test_update_with_patch_only);
stub!(test_interactive_mode_throws_if_no_package_to_update);
stub!(test_interactive_mode_throws_if_no_package_entered);
stub!(test_interactive_tmp);
stub!(test_no_security_blocking_allows_insecure_packages);
stub!(test_bump_after_update_without_lockfile);
