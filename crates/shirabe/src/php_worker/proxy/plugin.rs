//! `PluginInterface` adapter for a plugin entity living in the PHP child process.

use crate::event_dispatcher::{EventSubscriberInterface, SubscribedEventEntry};
use crate::php_worker::call::PhpCall;
use crate::php_worker::proxy::PhpProxy;
use crate::plugin::Capable;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_rpc::codec::DecodeError;
use shirabe_php_rpc::{FromRpcValue, PhpEntityHandle, RpcValue, codec};
use shirabe_php_shim::PhpMixed;

/// `PluginInterface` adapter for a plugin entity living in the PHP child process: every
/// lifecycle call is forwarded as a `CallMethod` RPC, by the `PluginInterface`,
/// `EventSubscriberInterface` and `Capable` impls `#[php_binding]` generates.
#[derive(Debug)]
pub struct PhpPluginProxy {
    pub(crate) handle: PhpEntityHandle,
}

impl PhpPluginProxy {
    pub fn new(handle: PhpEntityHandle) -> Self {
        Self { handle }
    }

    /// For testing only: reads a public property of the plugin entity in the child, mirroring
    /// PHPUnit assertions like `$plugins[0]->version`.
    pub fn __get_property(&self, name: &BStr) -> anyhow::Result<PhpMixed> {
        PhpCall::Function(BStr::new("__shirabe_get_property")).run(vec![
            RpcValue::PhpEntityHandle(self.handle.clone()),
            RpcValue::string(name),
        ])
    }

    // What the generated `PluginInterface`, `EventSubscriberInterface` and `Capable` impls
    // delegate the methods only the Rust side has to.
    pub(crate) fn get_class_name(&self) -> BString {
        self.handle.class().to_owned()
    }

    pub(crate) fn as_event_subscriber(&self) -> Option<&dyn EventSubscriberInterface> {
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

    pub(crate) fn as_capable(&self) -> Option<&dyn Capable> {
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

    pub(crate) fn __as_php_plugin_proxy(&self) -> Option<&PhpPluginProxy> {
        Some(self)
    }

    pub(crate) fn subscriber_handle(&self) -> PhpEntityHandle {
        self.handle.clone()
    }
}

impl PhpProxy for PhpPluginProxy {
    fn php_handle(&self) -> &PhpEntityHandle {
        &self.handle
    }
}

/// `getSubscribedEvents()`, which declares no return type: only the three shapes `addSubscriber`
/// reads decode.
pub(crate) fn subscribed_events(
    proxy: &PhpPluginProxy,
) -> anyhow::Result<IndexMap<BString, SubscribedEventEntry>> {
    // PHP: `$subscriber->getSubscribedEvents()` (an instance call of the static method).
    PhpCall::Method(&proxy.handle, BStr::new("getSubscribedEvents")).run(Vec::new())
}

/// `getCapabilities()`, which declares no return type.
pub(crate) fn capabilities(proxy: &PhpPluginProxy) -> anyhow::Result<IndexMap<BString, PhpMixed>> {
    let call = PhpCall::Method(&proxy.handle, BStr::new("getCapabilities"));
    let value: RpcValue = call.run(Vec::new())?;
    // PHP: `(array) $plugin->getCapabilities()` — the interface declares no return type,
    // so a non-array return is cast. A handle in the map (a plugin putting an object among
    // its capability values) has no PhpMixed image and fails the conversion explicitly.
    let entries = match value {
        RpcValue::Null => IndexMap::new(),
        array @ (RpcValue::Array(_) | RpcValue::List(_)) => {
            IndexMap::<BString, PhpMixed>::from_rpc_value(&array)
                .map_err(|error| call.unsupported_shape(error))?
        }
        scalar => IndexMap::from([(
            "0".into(),
            PhpMixed::from_rpc_value(&scalar).map_err(|error| call.unsupported_shape(error))?,
        )]),
    };
    Ok(entries)
}

/// One entry of a `getSubscribedEvents()` answer, in the three shapes `addSubscriber`
/// distinguishes: `'method'`, `['method', priority]`, and `[['method', priority], ...]`. A shape
/// outside these is an explicit error, never a silently dropped listener.
impl FromRpcValue for SubscribedEventEntry {
    fn from_rpc_value(value: &RpcValue) -> Result<Self, DecodeError> {
        let listener = |items: &[RpcValue]| -> Result<(BString, Option<i64>), DecodeError> {
            Ok((
                BString::from_rpc_value(items.first().unwrap_or(&codec::NULL))?,
                Option::<i64>::from_rpc_value(items.get(1).unwrap_or(&codec::NULL))?,
            ))
        };
        match value {
            RpcValue::String(method) => Ok(SubscribedEventEntry::Method(method.clone())),
            RpcValue::List(items) if matches!(items.first(), Some(RpcValue::String(_))) => {
                let (method, priority) = listener(items)?;
                Ok(SubscribedEventEntry::MethodWithPriority(method, priority))
            }
            RpcValue::List(items) => items
                .iter()
                .map(|item| match item {
                    RpcValue::List(pair) => listener(pair),
                    other => Err(codec::unexpected("a [method, priority] pair", other)),
                })
                .collect::<Result<_, _>>()
                .map(SubscribedEventEntry::Methods),
            other => Err(codec::unexpected(
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
        let events = IndexMap::<BString, SubscribedEventEntry>::from_rpc_value(&RpcValue::Array(
            IndexMap::from([
                (BString::from("a"), RpcValue::String(method.clone())),
                (
                    BString::from("b"),
                    RpcValue::List(vec![RpcValue::String(method.clone()), RpcValue::Int(1)]),
                ),
                (
                    BString::from("c"),
                    RpcValue::List(vec![RpcValue::List(vec![RpcValue::String(method.clone())])]),
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
        let events = IndexMap::<BString, SubscribedEventEntry>::from_rpc_value(&RpcValue::Array(
            IndexMap::from([(name.clone(), RpcValue::String(BString::from("method")))]),
        ))
        .unwrap();
        assert!(events.contains_key(&name));
    }
}
