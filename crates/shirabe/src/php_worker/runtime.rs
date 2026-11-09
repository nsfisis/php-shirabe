//! Thin wrappers that evaluate a PHP built-in in the worker on behalf of the ported code.

use crate::event_dispatcher::EventDispatcher;
use crate::php_worker::call::PhpCall;
use bstr::BStr;
use shirabe_php_rpc::{PhpObjHandle, PluginValue};

/// Runs a boolean class query (`class_exists`, `is_subclass_of`, ...) in the worker with the
/// script autoloader active, so a class named by `composer.json` resolves through the Rust-side
/// [`ClassLoader`]s.
pub(crate) fn php_class_query(function: &BStr, args: Vec<PluginValue>) -> anyhow::Result<bool> {
    EventDispatcher::ensure_script_autoloader()?;
    PhpCall::Function(function).run(args)
}

/// Instantiates `new $class(...$ctor_args)` in the worker, with the script autoloader active.
pub(crate) fn new_php_object(
    class: &BStr,
    ctor_args: Vec<PluginValue>,
) -> anyhow::Result<PhpObjHandle> {
    EventDispatcher::ensure_script_autoloader()?;
    PhpCall::New(class).run(ctor_args)
}

/// `is_a($obj, $class)` evaluated in the worker: the child's own class table answers, so
/// parent classes are covered (a `PhpObjHandle`'s `implements` lists interfaces only).
pub(crate) fn php_is_a(handle: &PhpObjHandle, class: &BStr) -> anyhow::Result<bool> {
    PhpCall::Function(BStr::new("is_a")).run(vec![
        PluginValue::PhpHandle(handle.clone()),
        PluginValue::string(class),
    ])
}
