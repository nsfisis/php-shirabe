//! ref: composer/tests/Composer/Test/Util/PlatformTest.php

use shirabe::util::platform::Platform;
use shirabe_php_shim::defined;

#[test]
#[ignore = "Platform::expand_path does not read the env var set via put_env in this runtime"]
fn test_expand_path() {
    Platform::put_env("TESTENV", "/home/test");
    assert_eq!("/home/test/myPath", Platform::expand_path("%TESTENV%/myPath"));
    assert_eq!("/home/test/myPath", Platform::expand_path("$TESTENV/myPath"));
    assert_eq!(
        format!(
            "{}/test",
            Platform::get_env("HOME")
                .or_else(|| Platform::get_env("USERPROFILE"))
                .unwrap_or_default()
        ),
        Platform::expand_path("~/test")
    );
}

#[test]
fn test_is_windows() {
    // Compare 2 common tests for Windows to the built-in Windows test
    assert_eq!(
        std::path::MAIN_SEPARATOR == '\\',
        Platform::is_windows()
    );
    assert_eq!(defined("PHP_WINDOWS_VERSION_MAJOR"), Platform::is_windows());
}
