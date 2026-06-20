//! ref: composer/tests/Composer/Test/Util/ErrorHandlerTest.php

// These tests rely on PHP's runtime error-handling machinery: ErrorHandler::register()
// installs a handler that converts notices/warnings into \ErrorException, and the tests
// trigger those by undefined-index access / array_merge misuse. There is no equivalent
// runtime mechanism in Rust to port faithfully.

#[test]
#[ignore = "relies on PHP's set_error_handler converting an undefined-array-key notice into an ErrorException; no Rust equivalent"]
fn test_error_handler_capture_notice() {
    todo!()
}

#[test]
#[ignore = "relies on PHP's runtime turning an invalid array_merge call into a TypeError/ErrorException; no Rust equivalent"]
fn test_error_handler_capture_warning() {
    todo!()
}

#[test]
#[ignore = "relies on PHP's @ error-suppression operator and set_error_handler; no Rust equivalent"]
fn test_error_handler_respects_at_operator() {
    todo!()
}
