//! `Capability` adapters for capability entities living in the PHP child process.

use crate::command::BaseCommand;
use crate::php_worker::call::PhpCall;
use crate::php_worker::proxy::PhpCommandProxy;
use crate::php_worker::rpc::entity::entity_value;
use crate::php_worker::runtime;
use crate::plugin::PluginInterface;
use crate::plugin::capability::{Capability, CommandProvider};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpObjHandle, PluginValue};
use shirabe_php_shim::{UnexpectedValueException, bformat};

/// `new $capabilityClass($ctorArgs)` with `$ctorArgs['plugin'] = $plugin`: the capability
/// constructor receives the plugin instance itself. A PHP-implemented plugin already has an
/// entity in the child's P table; a Rust-implemented one crosses as a handle to its R-table
/// entity.
pub(crate) fn new_capability_object(
    capability_class: &BStr,
    mut ctor_args: IndexMap<BString, PluginValue>,
    plugin: &std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
) -> anyhow::Result<PhpObjHandle> {
    let plugin_value = match plugin.borrow().__as_php_plugin_proxy() {
        Some(proxy) => PluginValue::PhpHandle(proxy.handle.clone()),
        None => entity_value(plugin)?,
    };
    ctor_args.insert(BString::from("plugin"), plugin_value);
    runtime::new_object(
        capability_class,
        vec![PluginValue::Array(ctor_args.into_iter().collect())],
    )
}

/// `Capability` adapter for a capability entity living in the PHP child process, for
/// capability interfaces that add no methods of their own (the plain
/// `Composer\Plugin\Capability\Capability` marker).
#[derive(Debug)]
pub struct PhpCapabilityProxy {
    handle: PhpObjHandle,
}

impl PhpCapabilityProxy {
    pub(crate) fn new(handle: PhpObjHandle) -> Self {
        Self { handle }
    }

    /// For testing only: the entity descriptor, whose class and interface list answer the
    /// `assertInstanceOf` checks PHPUnit makes on a capability.
    pub fn __handle(&self) -> &PhpObjHandle {
        &self.handle
    }

    /// For testing only: reads a public property of the capability entity in the child. Unlike
    /// `PhpPluginProxy::__get_property` the value keeps its plugin value form, so a test can assert
    /// the object identity behind a handle instead of only the plain data around it.
    pub fn __get_property(&self, name: &BStr) -> anyhow::Result<PluginValue> {
        PhpCall::Function(BStr::new("__shirabe_get_property")).run(vec![
            PluginValue::PhpHandle(self.handle.clone()),
            PluginValue::string(name),
        ])
    }
}

impl Capability for PhpCapabilityProxy {
    fn __as_php_capability_proxy(&self) -> Option<&PhpCapabilityProxy> {
        Some(self)
    }
}

/// `CommandProvider` adapter for a capability entity living in the PHP child process.
#[derive(Debug)]
pub struct PhpCommandProviderProxy {
    handle: PhpObjHandle,
}

impl PhpCommandProviderProxy {
    pub(crate) fn new(handle: PhpObjHandle) -> Self {
        Self { handle }
    }
}

impl Capability for PhpCommandProviderProxy {
    fn as_command_provider(&self) -> Option<&dyn CommandProvider> {
        Some(self)
    }
}

impl CommandProvider for PhpCommandProviderProxy {
    fn get_commands(
        &self,
    ) -> anyhow::Result<Vec<std::rc::Rc<std::cell::RefCell<dyn BaseCommand>>>> {
        let value: PluginValue =
            PhpCall::Method(&self.handle, BStr::new("getCommands")).run(Vec::new())?;
        // PHP's getCommands(): array is unenforceable on the wire, and a
        // `Vec<Box<dyn BaseCommand>>` asserts every element up front, so the two checks
        // Application::getPluginCommands performs on the raw value live here, with its
        // messages.
        let items = match value {
            PluginValue::List(items) => items,
            PluginValue::Array(map) => map.into_values().collect(),
            _ => {
                return Err(UnexpectedValueException::new(bformat!(
                    "Plugin capability {} failed to return an array from getCommands",
                    self.handle.class()
                ))
                .into());
            }
        };
        let mut commands: Vec<std::rc::Rc<std::cell::RefCell<dyn BaseCommand>>> = Vec::new();
        for item in items {
            let command_handle = match item {
                PluginValue::PhpHandle(handle) => handle,
                _ => return Err(invalid_command_error(&self.handle)),
            };
            if !runtime::is_a(&command_handle, BStr::new("Composer\\Command\\BaseCommand"))? {
                return Err(invalid_command_error(&self.handle));
            }
            commands.push(std::rc::Rc::new(std::cell::RefCell::new(
                PhpCommandProxy::new(command_handle)?,
            )));
        }
        Ok(commands)
    }
}

fn invalid_command_error(capability: &PhpObjHandle) -> anyhow::Error {
    UnexpectedValueException::new(bformat!("Plugin capability {} returned an invalid value, we expected an array of Composer\\Command\\BaseCommand objects", capability.class()))
    .into()
}
