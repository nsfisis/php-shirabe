//! ref: composer/tests/Composer/Test/Command/BumpCommandTest.php

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_bump);
stub!(test_bump_fails_on_non_existing_composer_file);
stub!(test_bump_fails_on_write_error_to_composer_file);
