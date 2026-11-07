//! Shared access to the PHP worker for integration tests.
//!
//! Included into an integration-test binary via
//! `#[path = "../common/php_worker.rs"] mod php_worker;`.
#![allow(dead_code)]

use bstr::BStr;
use shirabe::plugin::php_call::PhpCall;
use shirabe_php_rpc::PluginValue;

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
pub fn php_call_static(
    class: impl AsRef<[u8]>,
    method: impl AsRef<[u8]>,
    args: Vec<PluginValue>,
) -> PluginValue {
    let class = BStr::new(class.as_ref());
    let method = BStr::new(method.as_ref());
    PhpCall::StaticMethod(class, method)
        .try_run::<PluginValue>(args)
        .unwrap_or_else(|error| {
            panic!(
                "{}::{} request failed: {error}",
                class.display_lossy(),
                method.display_lossy()
            )
        })
        .unwrap_or_else(|throw| {
            panic!(
                "{}::{} threw {throw}",
                class.display_lossy(),
                method.display_lossy()
            )
        })
}

/// Runs a PHP snippet in the worker and returns its `return` value.
pub fn php_eval(code: impl AsRef<[u8]>) -> PluginValue {
    PhpCall::Function(BStr::new("__shirabe_eval"))
        .try_run(vec![PluginValue::string(code.as_ref())])
        .expect("eval request failed")
        .expect("eval threw")
}
