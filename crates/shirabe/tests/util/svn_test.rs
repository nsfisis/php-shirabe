//! ref: composer/tests/Composer/Test/Util/SvnTest.php

// These mock IO/Config and use reflection to drive Svn's credential handling; mocking and
// reflection are not available here.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks IO/Config and uses reflection to drive Svn credentials; not ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_credentials);
stub!(test_interactive_string);
stub!(test_credentials_from_config);
stub!(test_credentials_from_config_with_cache_credentials_true);
stub!(test_credentials_from_config_with_cache_credentials_false);
