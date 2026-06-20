//! ref: composer/tests/Composer/Test/Command/SuggestsCommandTest.php

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_installed_packages_with_no_suggestions);
stub!(test_suggest);
