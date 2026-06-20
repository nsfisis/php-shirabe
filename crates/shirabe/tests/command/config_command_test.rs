//! ref: composer/tests/Composer/Test/Command/ConfigCommandTest.php

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_config_updates);
stub!(test_config_reads);
stub!(test_config_throws_for_invalid_arg_combination);
stub!(test_config_throws_for_invalid_severity);
stub!(test_config_throws_when_merging_array_with_object);
