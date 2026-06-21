//! ref: composer/tests/Composer/Test/Util/ErrorHandlerTest.php

// These tests rely on PHP's runtime error-handling machinery: ErrorHandler::register()
// installs a handler that converts notices/warnings into \ErrorException, and the tests
// trigger those by undefined-index access / array_merge misuse. There is no equivalent
// runtime mechanism in Rust to port faithfully.

#[allow(dead_code)]
fn set_up() {
    // ErrorHandler::register() installs a PHP set_error_handler; no Rust equivalent.
    todo!()
}

#[allow(dead_code)]
fn tear_down() {
    // restore_error_handler() is PHP runtime machinery; no Rust equivalent.
    todo!()
}

#[allow(dead_code)]
struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

#[ignore = "depends on PHP runtime routing an undefined-index notice through set_error_handler; no Rust equivalent for $array['baz'] triggering ErrorHandler::handle"]
#[test]
fn test_error_handler_capture_notice() {
    todo!()
}

#[ignore = "depends on PHP runtime emitting a TypeError/warning from array_merge([], 'string') via set_error_handler; no Rust equivalent"]
#[test]
fn test_error_handler_capture_warning() {
    todo!()
}

#[ignore = "depends on the PHP @ error-suppression operator and trigger_error routing through set_error_handler; no Rust equivalent"]
#[test]
fn test_error_handler_respects_at_operator() {
    todo!()
}
