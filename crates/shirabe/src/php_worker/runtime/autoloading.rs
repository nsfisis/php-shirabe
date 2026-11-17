//! Wrappers that turn the script autoloader on before the call, so a class named by
//! `composer.json` resolves through the Rust-side [`ClassLoader`]s.
//!
//! [`ClassLoader`]: crate::autoload::ClassLoader

use crate::php_worker::call::PhpCall;
use crate::php_worker::runtime::ensure_script_autoloader;
use bstr::BStr;
use shirabe_php_rpc::{PhpEntityHandle, RpcValue};

/// `class_exists($class)`.
pub(crate) fn class_exists(class: &BStr) -> anyhow::Result<bool> {
    ensure_script_autoloader()?;
    PhpCall::Function(BStr::new("class_exists")).run(vec![RpcValue::string(class)])
}

/// `is_subclass_of($class, $parent, true)`.
pub(crate) fn is_subclass_of(class: &BStr, parent: &BStr) -> anyhow::Result<bool> {
    ensure_script_autoloader()?;
    PhpCall::Function(BStr::new("is_subclass_of")).run(vec![
        RpcValue::string(class),
        RpcValue::string(parent),
        RpcValue::Bool(true),
    ])
}

/// `new $class(...$args)`.
pub(crate) fn new_object(class: &BStr, args: Vec<RpcValue>) -> anyhow::Result<PhpEntityHandle> {
    ensure_script_autoloader()?;
    PhpCall::New(class).run(args)
}

/// `is_callable($callable)` for a `Class::method` script.
pub(crate) fn is_callable(callable: &BStr) -> anyhow::Result<bool> {
    ensure_script_autoloader()?;
    PhpCall::Function(BStr::new("is_callable")).run(vec![RpcValue::string(callable)])
}

/// `is_a($class, $parent, true)`.
pub(crate) fn is_a_class(class: &BStr, parent: &BStr) -> anyhow::Result<bool> {
    ensure_script_autoloader()?;
    PhpCall::Function(BStr::new("is_a")).run(vec![
        RpcValue::string(class),
        RpcValue::string(parent),
        RpcValue::Bool(true),
    ])
}

/// `defined($name)`.
pub(crate) fn defined(name: &BStr) -> anyhow::Result<bool> {
    ensure_script_autoloader()?;
    PhpCall::Function(BStr::new("defined")).run(vec![RpcValue::string(name)])
}
