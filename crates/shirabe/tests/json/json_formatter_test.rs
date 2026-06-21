//! ref: composer/tests/Composer/Test/Json/JsonFormatterTest.php

/// Test if ę will get correctly formatted (unescaped)
/// https://github.com/composer/composer/issues/2613
#[ignore = "JsonFormatter is not implemented in crates/shirabe/src/json"]
#[test]
fn test_unicode_with_prepended_slash() {
    todo!()
}

/// Surrogate pairs are intentionally skipped and not unescaped
/// https://github.com/composer/composer/issues/7510
#[ignore = "JsonFormatter is not implemented in crates/shirabe/src/json"]
#[test]
fn test_utf16_surrogate_pair() {
    todo!()
}
