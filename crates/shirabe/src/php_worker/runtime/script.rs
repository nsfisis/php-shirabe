//! Wrappers for the queries `EventDispatcher` makes about a script: they turn the script
//! autoloader on and serve the callbacks with the [`ScriptRpcDispatcher`].

use crate::php_worker::call::PhpCall;
use crate::php_worker::rpc::dispatch::ScriptRpcDispatcher;
use crate::php_worker::runtime::ensure_script_autoloader;
use bstr::BStr;
use shirabe_php_rpc::PluginValue;

/// Runs a boolean runtime query inside the PHP worker, with the script autoloader active so the
/// query can trigger class loading.
fn query(function: &BStr, args: Vec<PluginValue>) -> anyhow::Result<bool> {
    ensure_script_autoloader()?;
    PhpCall::Function(function).run_with(args, &mut ScriptRpcDispatcher)
}

/// `class_exists($class)`.
pub(crate) fn class_exists(class: &BStr) -> anyhow::Result<bool> {
    query(BStr::new("class_exists"), vec![PluginValue::string(class)])
}

/// `is_callable($callable)` for a `Class::method` script.
pub(crate) fn is_callable(callable: &BStr) -> anyhow::Result<bool> {
    query(
        BStr::new("is_callable"),
        vec![PluginValue::string(callable)],
    )
}

/// `is_a($class, $parent, true)`.
pub(crate) fn is_a_class(class: &BStr, parent: &BStr) -> anyhow::Result<bool> {
    query(
        BStr::new("is_a"),
        vec![
            PluginValue::string(class),
            PluginValue::string(parent),
            PluginValue::Bool(true),
        ],
    )
}

/// `defined($name)`.
pub(crate) fn defined(name: &BStr) -> anyhow::Result<bool> {
    query(BStr::new("defined"), vec![PluginValue::string(name)])
}
