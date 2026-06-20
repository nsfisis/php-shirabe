//! ref: composer/tests/Composer/Test/Repository/PlatformRepositoryTest.php

// These probe runtime/extension/library versions and assert the synthesized platform
// packages; the detection mocks ProcessExecutor/Runtime/HhvmDetector and the package
// versions are parsed through a look-around regex.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "not yet ported (platform detection mocks Runtime/ProcessExecutor; version parsing uses a look-around regex)"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_hhvm_package);
stub!(test_php_version);
stub!(test_inet_pton_regression);
stub!(test_library_information);
stub!(test_composer_platform_version);
stub!(test_valid_platform_packages);
