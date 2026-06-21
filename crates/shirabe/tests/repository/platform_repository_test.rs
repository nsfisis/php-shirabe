//! ref: composer/tests/Composer/Test/Repository/PlatformRepositoryTest.php

use shirabe::repository::PlatformRepository;

// These probe runtime/extension/library versions and assert the synthesized platform
// packages; the detection mocks ProcessExecutor/Runtime/HhvmDetector and the package
// versions are parsed through a look-around regex.
#[test]
#[ignore = "requires PHPUnit getMockBuilder mock of Composer\\Platform\\HhvmDetector (willReturn getVersion); Runtime/HhvmDetector are concrete structs with no mock/override mechanism"]
fn test_hhvm_package() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit getMockBuilder mock of Composer\\Platform\\Runtime (willReturnCallback/willReturnMap for hasConstant/getConstant/invoke/getExtensions); no mocking framework and Runtime is a concrete struct"]
fn test_php_version() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit getMockBuilder mock of Composer\\Platform\\Runtime (expects(once)/with/willReturn for invoke, willReturnCallback for getConstant); no mocking framework"]
fn test_inet_pton_regression() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit getMockBuilder mock of Composer\\Platform\\Runtime (willReturnMap/willReturnCallback for getExtensions/getExtensionVersion/getExtensionInfo/invoke/hasConstant/getConstant/hasClass/construct) plus ResourceBundleStub/ImagickStub; no mocking framework"]
fn test_library_information() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit getMockBuilder mock of Composer\\Platform\\Runtime (willReturnMap for getConstant/getExtensions); no mocking framework and Runtime is a concrete struct"]
fn test_composer_platform_version() {
    todo!()
}

#[test]
fn test_valid_platform_packages() {
    let cases: Vec<(&str, bool)> = vec![
        ("php", true),
        ("php-debug", true),
        ("php-ipv6", true),
        ("php-64bit", true),
        ("php-zts", true),
        ("hhvm", true),
        ("hhvm-foo", false),
        ("ext-foo", true),
        ("ext-123", true),
        ("extfoo", false),
        ("ext", false),
        ("lib-foo", true),
        ("lib-123", true),
        ("libfoo", false),
        ("lib", false),
        ("composer", true),
        ("composer-foo", false),
        ("composer-plugin-api", true),
        ("composer-plugin", false),
        ("composer-runtime-api", true),
        ("composer-runtime", false),
    ];

    for (package_name, expectation) in cases {
        assert_eq!(
            expectation,
            PlatformRepository::is_platform_package(package_name)
        );
    }
}
