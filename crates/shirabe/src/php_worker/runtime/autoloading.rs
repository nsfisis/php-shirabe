//! Wrappers that turn the script autoloader on before the call, so a class named by
//! `composer.json` resolves through the Rust-side [`ClassLoader`]s.
//!
//! [`ClassLoader`]: crate::autoload::ClassLoader

use crate::php_worker::call::PhpCall;
use crate::php_worker::runtime::ensure_script_autoloader;
use bstr::BStr;
use shirabe_php_rpc::{PhpObjHandle, PluginValue};

/// `class_exists($class)`.
pub(crate) fn class_exists(class: &BStr) -> anyhow::Result<bool> {
    ensure_script_autoloader()?;
    PhpCall::Function(BStr::new("class_exists")).run(vec![PluginValue::string(class)])
}

/// `is_subclass_of($class, $parent, true)`.
pub(crate) fn is_subclass_of(class: &BStr, parent: &BStr) -> anyhow::Result<bool> {
    ensure_script_autoloader()?;
    PhpCall::Function(BStr::new("is_subclass_of")).run(vec![
        PluginValue::string(class),
        PluginValue::string(parent),
        PluginValue::Bool(true),
    ])
}

/// `new $class(...$args)`.
pub(crate) fn new_object(class: &BStr, args: Vec<PluginValue>) -> anyhow::Result<PhpObjHandle> {
    ensure_script_autoloader()?;
    PhpCall::New(class).run(args)
}
