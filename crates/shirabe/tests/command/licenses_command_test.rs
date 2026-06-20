//! ref: composer/tests/Composer/Test/Command/LicensesCommandTest.php

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_basic_run);
stub!(test_no_dev);
stub!(test_format_json);
stub!(test_format_summary);
stub!(test_format_unknown);
stub!(test_locked);
stub!(test_locked_no_dev);
stub!(test_locked_without_lock_file);
