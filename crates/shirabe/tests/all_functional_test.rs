//! ref: composer/tests/Composer/Test/AllFunctionalTest.php

// These build the composer.phar and run the .test integration fixtures by invoking the
// composer binary as a subprocess; the phar build and functional-test harness are not
// ported.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "not yet ported (builds composer.phar and runs the functional .test fixtures via the binary)"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_build_phar);
stub!(test_integration);
