//! Rust adapters for entities living in the PHP child process: a plugin, an installer, a
//! capability, a command. Each holds the entity's P-table handle and forwards the Rust trait it
//! implements over RPC. This module has no Composer counterpart.

mod capability;
mod command;
mod installer;
mod plugin;

pub use capability::*;
pub use command::*;
pub use installer::*;
pub use plugin::*;

use crate::plugin::php_call::PhpCall;
use bstr::BStr;
use shirabe_php_rpc::{PhpObjHandle, PluginValue};

/// Runs a boolean class query (`class_exists`, `is_subclass_of`, ...) in the worker with the
/// script autoloader active, so a class named by `composer.json` resolves through the Rust-side
/// [`ClassLoader`]s.
pub(crate) fn php_class_query(function: &BStr, args: Vec<PluginValue>) -> anyhow::Result<bool> {
    crate::event_dispatcher::EventDispatcher::ensure_script_autoloader()?;
    PhpCall::Function(function).run(args)
}

/// Instantiates `new $class(...$ctor_args)` in the worker, with the script autoloader active.
pub(crate) fn new_php_object(
    class: &BStr,
    ctor_args: Vec<PluginValue>,
) -> anyhow::Result<PhpObjHandle> {
    crate::event_dispatcher::EventDispatcher::ensure_script_autoloader()?;
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
