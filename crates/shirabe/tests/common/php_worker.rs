//! Shared access to the PHP worker for integration tests.
//!
//! Included into an integration-test binary via
//! `#[path = "../common/php_worker.rs"] mod php_worker;`.
#![allow(dead_code)]

use shirabe_php_rpc::PluginValue;
use shirabe_symfony_process::PhpExecutableFinder;

/// Whether a PHP binary is available. Without one the worker cannot start, so tests that need it
/// return early instead of failing.
pub fn php_runtime_available() -> bool {
    PhpExecutableFinder::new().find(false).is_some()
}

/// All tests in one binary share the single PHP worker, whose loaded-class table and class statics
/// persist across tests just like PHPUnit's single-process runs. Interleaving two tests would let
/// one test's state race the other's, so the worker-touching tests run serialized.
static PHP_WORKER_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn lock_php_worker() -> std::sync::MutexGuard<'static, ()> {
    PHP_WORKER_TESTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Requires the Composer PHP runtime's `vendor/autoload.php` into the worker, which is what makes
/// the real `Composer\` classes autoloadable there. A worker whose PHP cannot read the runtime out
/// of the test binary unpacks it under a cache directory of this test run, never the one the
/// developer's own Composer uses.
pub fn load_composer_php_runtime() {
    static CACHE: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| tempfile::tempdir().expect("no cache directory for the test"));
    shirabe::event_dispatcher::EventDispatcher::__ensure_composer_php_runtime(cache.path())
        .unwrap();
}

/// Calls a static method in the worker, panicking on either failure lane.
pub fn php_call_static(class: &str, method: &str, args: Vec<PluginValue>) -> PluginValue {
    shirabe_php_rpc::call_static_method(class, method, args, None)
        .unwrap_or_else(|error| panic!("{class}::{method} request failed: {error}"))
        .unwrap_or_else(|throw| {
            panic!(
                "{class}::{method} threw {}: {}",
                throw.exception_class, throw.message
            )
        })
}

/// Runs a PHP snippet in the worker and returns its `return` value.
pub fn php_eval(code: &str) -> PluginValue {
    shirabe_php_rpc::call_function("__shirabe_eval", vec![PluginValue::string(code)])
        .expect("eval request failed")
        .expect("eval threw")
}

/// Quotes a string as a PHP single-quoted literal for a generated snippet.
pub fn php_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}
