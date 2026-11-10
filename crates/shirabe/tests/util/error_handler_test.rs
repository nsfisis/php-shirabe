//! ref: composer/tests/Composer/Test/Util/ErrorHandlerTest.php sha1:556b731ac1584d066b73f8857939822da878aa84
//!
//! `ErrorHandler` runs in the PHP worker, where plugin and script code raises PHP errors. These
//! tests register it through the Rust-side `ErrorHandler::register` and run the body of each
//! upstream test method there, so the exception PHPUnit would catch comes back over the wire.

use crate::php_worker::{load_composer_php_runtime, lock_php_worker, php_eval};
use bstr::{BStr, ByteSlice as _};
use shirabe::php_worker::call::PhpCall;
use shirabe::php_worker::error_handler::restore_error_handler;
use shirabe::util::ErrorHandler;
use shirabe_php_rpc::{PhpThrow, PluginValue};

fn set_up() -> TearDown {
    load_composer_php_runtime();
    ErrorHandler::register(None).unwrap();
    TearDown
}

fn tear_down() {
    restore_error_handler().unwrap();
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

fn php_version_id() -> i64 {
    match php_eval("return \\PHP_VERSION_ID;") {
        PluginValue::Int(id) => id,
        other => panic!("expected an int, got {other:?}"),
    }
}

/// Runs a PHP snippet in the worker, expecting it to throw.
fn php_eval_throw(code: impl AsRef<[u8]>) -> PhpThrow {
    match PhpCall::Function(BStr::new("__shirabe_eval"))
        .try_run::<PluginValue>(vec![PluginValue::string(code.as_ref())])
        .expect("eval request failed")
    {
        Ok(value) => panic!("expected an exception, got {value:?}"),
        Err(throw) => throw,
    }
}

/// `expectException($class)` and `expectExceptionMessage($message)` checked against `throw`.
fn assert_exception(throw: &PhpThrow, class: &BStr, message: &BStr) {
    let class = class.trim_start_with(|c| c == '\\').as_bstr();
    assert!(
        throw.exception_class == class || throw.parents.iter().any(|parent| parent == class),
        "expected an instance of {}, got {throw}",
        class.display_lossy()
    );
    assert!(
        throw.message.contains_str(message),
        "expected the message of {throw} to contain {message:?}"
    );
}

/// Test ErrorHandler handles notices
#[test]
fn test_error_handler_capture_notice() {
    let _worker = lock_php_worker();
    let _tear_down = set_up();

    let (class, message) = if php_version_id() >= 80000 {
        (
            BStr::new("\\ErrorException"),
            BStr::new("Undefined array key \"baz\""),
        )
    } else {
        (
            BStr::new("\\ErrorException"),
            BStr::new("Undefined index: baz"),
        )
    };

    let throw = php_eval_throw(
        "$array = ['foo' => 'bar'];
        $array['baz'];",
    );
    assert_exception(&throw, class, message);
}

/// Test ErrorHandler handles warnings
#[test]
fn test_error_handler_capture_warning() {
    let _worker = lock_php_worker();
    let _tear_down = set_up();

    let (class, message) = if php_version_id() >= 80000 {
        (BStr::new("TypeError"), BStr::new("array_merge"))
    } else {
        (BStr::new("ErrorException"), BStr::new("array_merge"))
    };

    let throw = php_eval_throw("array_merge([], 'string');");
    assert_exception(&throw, class, message);
}

/// Test ErrorHandler handles warnings
#[test]
fn test_error_handler_respects_at_operator() {
    let _worker = lock_php_worker();
    let _tear_down = set_up();

    php_eval("@trigger_error('test', E_USER_NOTICE);");
}
