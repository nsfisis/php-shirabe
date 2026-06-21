//! ref: composer/tests/Composer/Test/Command/LicensesCommandTest.php

fn set_up() {
    // Builds the temp project and installed.json/composer.lock fixtures via
    // initTempComposer/createInstalledJson/createComposerLock, none of which are ported yet.
    todo!()
}

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
        fn $name() {
            set_up();

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
