//! ref: composer/tests/Composer/Test/Util/PlatformTest.php sha1:b857dacddc127df7cc5c28da628085154a794f8f

use shirabe::util::Platform;
use shirabe_php_shim::{bformat, defined};

#[test]
fn test_expand_path() {
    Platform::put_env("TESTENV", "/home/test").unwrap();
    assert_eq!(
        "/home/test/myPath",
        Platform::expand_path("%TESTENV%/myPath")
    );
    assert_eq!(
        "/home/test/myPath",
        Platform::expand_path("$TESTENV/myPath")
    );
    assert_eq!(
        bformat!(
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
    assert_eq!(cfg!(windows), Platform::is_windows());
    assert_eq!(defined("PHP_WINDOWS_VERSION_MAJOR"), Platform::is_windows());
}
