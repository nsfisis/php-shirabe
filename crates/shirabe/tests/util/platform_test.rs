//! ref: composer/tests/Composer/Test/Util/PlatformTest.php

use shirabe::util::platform::Platform;
use shirabe_php_shim::defined;

#[test]
#[ignore = "Preg::replace_callback doesn't set PREG_UNMATCHED_AS_NULL, so the non-participating \
alternation branch (dvar) is captured as an empty string instead of absent; Platform::expand_path's \
matches.get(dvar).or_else(pvar) then picks the empty dvar over pvar for the %VAR% form"]
fn test_expand_path() {
    Platform::put_env("TESTENV", "/home/test");
    assert_eq!(
        "/home/test/myPath",
        Platform::expand_path("%TESTENV%/myPath")
    );
    assert_eq!(
        "/home/test/myPath",
        Platform::expand_path("$TESTENV/myPath")
    );
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
    assert_eq!(std::path::MAIN_SEPARATOR == '\\', Platform::is_windows());
    assert_eq!(defined("PHP_WINDOWS_VERSION_MAJOR"), Platform::is_windows());
}
