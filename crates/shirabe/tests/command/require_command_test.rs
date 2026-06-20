//! ref: composer/tests/Composer/Test/Command/RequireCommandTest.php

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_require_throws_if_none_matches);
stub!(test_require_warns_if_resolved_to_feature_branch);
stub!(test_require);
stub!(test_inconsistent_require_keys);
