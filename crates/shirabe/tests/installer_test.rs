//! ref: composer/tests/Composer/Test/InstallerTest.php

// These are large end-to-end installer integration cases driven by fixtures and a mocked
// Composer/IO/repositories; the full install pipeline (and constraint parsing through a
// look-around regex) is not ported.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "not yet ported (end-to-end Installer integration over fixtures; constraint parsing uses a look-around regex)"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_installer);
stub!(test_slow_integration);
stub!(test_integration_with_pool_optimizer);
stub!(test_integration_with_raw_pool);
