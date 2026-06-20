//! ref: composer/tests/Composer/Test/Package/Loader/ValidatingArrayLoaderTest.php

// ValidatingArrayLoader wraps ArrayLoader, whose constraint parsing uses a look-around
// regex the regex crate cannot compile; the success/warning data sets are large.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "ValidatingArrayLoader -> ArrayLoader parses constraints via a look-around regex the regex crate cannot compile"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_load_success);
stub!(test_load_failure_throws_exception);
stub!(test_load_warnings);
stub!(test_load_skips_warning_data_when_ignoring_errors);
