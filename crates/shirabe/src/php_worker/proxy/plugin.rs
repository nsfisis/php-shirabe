//! `PluginInterface` adapter for a plugin entity living in the PHP child process.

use crate::composer::ComposerHandle;
use crate::event_dispatcher::{EventSubscriberInterface, SubscribedEventEntry};
use crate::io::IOInterface;
use crate::php_worker::call::PhpCall;
use crate::php_worker::rpc::table::{composer_handle_value, io_handle_value};
use crate::plugin::{Capable, PluginInterface};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_rpc::{FromWire, PhpObjHandle, PhpThrow, PluginValue, wire};
use shirabe_php_shim::PhpMixed;

/// `PluginInterface` adapter for a plugin entity living in the PHP child process: every
/// lifecycle call is forwarded as a `CallPhpMethod` RPC.
#[derive(Debug)]
pub struct PhpPluginProxy {
    pub(crate) handle: PhpObjHandle,
}

impl PhpPluginProxy {
    pub fn new(handle: PhpObjHandle) -> Self {
        Self { handle }
    }

    fn forward_lifecycle_call(
        &self,
        method: &BStr,
        composer: &ComposerHandle,
        io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        let args = vec![composer_handle_value(composer), io_handle_value(io)?];
        PhpCall::Method(&self.handle, method).run::<PluginValue>(args)?;
        Ok(())
    }

    /// For testing only: reads a public property of the plugin entity in the child, mirroring
    /// PHPUnit assertions like `$plugins[0]->version`.
    pub fn __get_property(&self, name: &BStr) -> anyhow::Result<PhpMixed> {
        PhpCall::Function(BStr::new("__shirabe_get_property")).run(vec![
            PluginValue::PhpHandle(self.handle.clone()),
            PluginValue::string(name),
        ])
    }
}

impl PluginInterface for PhpPluginProxy {
    fn activate(
        &mut self,
        composer: ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        self.forward_lifecycle_call(BStr::new("activate"), &composer, &io)
    }

    fn deactivate(
        &mut self,
        composer: ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        self.forward_lifecycle_call(BStr::new("deactivate"), &composer, &io)
    }

    fn uninstall(
        &mut self,
        composer: ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        self.forward_lifecycle_call(BStr::new("uninstall"), &composer, &io)
    }

    fn get_class_name(&self) -> BString {
        self.handle.class().to_owned()
    }

    fn as_event_subscriber(&self) -> Option<&dyn EventSubscriberInterface> {
        if self
            .handle
            .implements()
            .iter()
            .any(|interface| interface == "Composer\\EventDispatcher\\EventSubscriberInterface")
        {
            Some(self)
        } else {
            None
        }
    }

    fn as_capable(&self) -> Option<&dyn Capable> {
        if self
            .handle
            .implements()
            .iter()
            .any(|interface| interface == "Composer\\Plugin\\Capable")
        {
            Some(self)
        } else {
            None
        }
    }

    fn __as_php_plugin_proxy(&self) -> Option<&PhpPluginProxy> {
        Some(self)
    }
}

impl EventSubscriberInterface for PhpPluginProxy {
    fn get_subscribed_events(&self) -> anyhow::Result<IndexMap<BString, SubscribedEventEntry>> {
        // PHP: `$subscriber->getSubscribedEvents()` (an instance call of the static method).
        PhpCall::Method(&self.handle, BStr::new("getSubscribedEvents")).run(Vec::new())
    }

    fn subscriber_handle(&self) -> PhpObjHandle {
        self.handle.clone()
    }
}

impl Capable for PhpPluginProxy {
    fn get_capabilities(&self) -> anyhow::Result<IndexMap<BString, PhpMixed>> {
        let call = PhpCall::Method(&self.handle, BStr::new("getCapabilities"));
        let value: PluginValue = call.run(Vec::new())?;
        // PHP: `(array) $plugin->getCapabilities()` — the interface declares no return type,
        // so a non-array return is cast. A handle in the map (a plugin putting an object among
        // its capability values) has no PhpMixed image and fails the conversion explicitly.
        let entries = match value {
            PluginValue::Null => IndexMap::new(),
            array @ (PluginValue::Array(_) | PluginValue::List(_)) => {
                IndexMap::<BString, PhpMixed>::from_wire(&array)
                    .map_err(|throw| call.unsupported_shape(throw))?
            }
            scalar => IndexMap::from([(
                "0".into(),
                PhpMixed::from_wire(&scalar).map_err(|throw| call.unsupported_shape(throw))?,
            )]),
        };
        Ok(entries)
    }
}

/// One entry of a `getSubscribedEvents()` answer, in the three shapes `addSubscriber`
/// distinguishes: `'method'`, `['method', priority]`, and `[['method', priority], ...]`. A shape
/// outside these is an explicit error, never a silently dropped listener.
impl FromWire for SubscribedEventEntry {
    fn from_wire(value: &PluginValue) -> Result<Self, PhpThrow> {
        let listener = |items: &[PluginValue]| -> Result<(BString, Option<i64>), PhpThrow> {
            Ok((
                BString::from_wire(items.first().unwrap_or(&wire::NULL))?,
                Option::<i64>::from_wire(items.get(1).unwrap_or(&wire::NULL))?,
            ))
        };
        match value {
            PluginValue::String(method) => Ok(SubscribedEventEntry::Method(method.clone())),
            PluginValue::List(items) if matches!(items.first(), Some(PluginValue::String(_))) => {
                let (method, priority) = listener(items)?;
                Ok(SubscribedEventEntry::MethodWithPriority(method, priority))
            }
            PluginValue::List(items) => items
                .iter()
                .map(|item| match item {
                    PluginValue::List(pair) => listener(pair),
                    other => Err(wire::unexpected("a [method, priority] pair", other)),
                })
                .collect::<Result<_, _>>()
                .map(SubscribedEventEntry::Methods),
            other => Err(wire::unexpected(
                "a listener method or a list of them",
                other,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listener_method_name_that_is_not_utf8_keeps_its_bytes() {
        let method = BString::from(b"on\xffEvent".as_slice());
        let events = IndexMap::<BString, SubscribedEventEntry>::from_wire(&PluginValue::Array(
            IndexMap::from([
                (BString::from("a"), PluginValue::String(method.clone())),
                (
                    BString::from("b"),
                    PluginValue::List(vec![
                        PluginValue::String(method.clone()),
                        PluginValue::Int(1),
                    ]),
                ),
                (
                    BString::from("c"),
                    PluginValue::List(vec![PluginValue::List(vec![PluginValue::String(
                        method.clone(),
                    )])]),
                ),
            ]),
        ))
        .unwrap();
        assert!(matches!(&events[bstr::B("a")], SubscribedEventEntry::Method(m) if *m == method));
        assert!(
            matches!(&events[bstr::B("b")], SubscribedEventEntry::MethodWithPriority(m, Some(1)) if *m == method)
        );
        assert!(
            matches!(&events[bstr::B("c")], SubscribedEventEntry::Methods(l) if l[0].0 == method)
        );
    }

    #[test]
    fn an_event_name_that_is_not_utf8_keeps_its_bytes() {
        let name = BString::from(b"event\xE9".as_slice());
        let events = IndexMap::<BString, SubscribedEventEntry>::from_wire(&PluginValue::Array(
            IndexMap::from([(name.clone(), PluginValue::String(BString::from("method")))]),
        ))
        .unwrap();
        assert!(events.contains_key(&name));
    }
}
