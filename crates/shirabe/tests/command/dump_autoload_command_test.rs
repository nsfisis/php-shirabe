//! ref: composer/tests/Composer/Test/Command/DumpAutoloadCommandTest.php

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_dump_autoload);
stub!(test_dump_dev_autoload);
stub!(test_dump_no_dev_autoload);
stub!(test_using_optimize_and_strict_psr);
stub!(test_fails_using_strict_psr_if_class_map_violations_are_found);
stub!(test_using_classmap_authoritative);
stub!(test_using_classmap_authoritative_and_strict_psr);
stub!(test_strict_psr_does_not_work_without_optimized_autoloader);
stub!(test_dev_and_no_dev_cannot_be_combined);
stub!(test_with_custom_autoloader_suffix);
stub!(test_with_existing_composer_lock_and_autoloader_suffix);
stub!(test_with_existing_composer_lock_without_autoloader_suffix);
