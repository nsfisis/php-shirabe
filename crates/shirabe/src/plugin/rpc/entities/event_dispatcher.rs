//! The methods a `Composer\EventDispatcher\EventDispatcher` entity answers over RPC.

use crate::event_dispatcher::{Event, EventDispatcherInterface};
use crate::plugin::php_call::PhpCall;
use crate::plugin::rpc::codec::arg;
use crate::plugin::rpc::dispatch::unavailable_method;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_rpc::{PhpThrow, PluginValue};
use shirabe_php_shim::bformat;

/// PHP's `is_callable($value)`, answered by the worker because only its own function and class
/// tables can say whether a string or a `[$object, 'method']` pair names something callable.
/// Values that cannot name a callable at all are answered here rather than over a round trip.
pub(crate) fn is_php_callable(value: &PluginValue) -> anyhow::Result<bool> {
    match value {
        PluginValue::String(_)
        | PluginValue::List(_)
        | PluginValue::Array(_)
        | PluginValue::PhpHandle(_) => {
            PhpCall::Function(BStr::new("is_callable")).run(vec![value.clone()])
        }
        _ => Ok(false),
    }
}

/// Calls a PHP callable value in the worker, whatever form it takes: `call_user_func` resolves a
/// closure, a function name and an `[$object, 'method']` pair exactly as the original call site
/// would have.
pub(crate) fn call_php_callable(
    callable: &PluginValue,
    args: Vec<PluginValue>,
) -> anyhow::Result<PluginValue> {
    let mut call_args = Vec::with_capacity(args.len() + 1);
    call_args.push(callable.clone());
    call_args.extend(args);
    PhpCall::Function(BStr::new("call_user_func")).run(call_args)
}

pub(crate) fn dispatch_event_dispatcher_method(
    dispatcher: &std::rc::Rc<std::cell::RefCell<dyn EventDispatcherInterface>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"dispatch" => {
            let name = arg::<BString>(method_name, args, 0)?;
            let probe = Event::from_name(name.clone());
            if dispatcher.borrow_mut().has_event_listeners(&probe) {
                // TODO(plugin): dispatching a worker-constructed event through the Rust-side
                // dispatcher needs the event object (and the console input it carries) proxied
                // back into this process, so only the no-listener case is answered here.
                return Err(PhpThrow::runtime(bformat!(
                    "dispatching `{name}` from the plugin process is not supported yet while listeners are registered for it"
                )));
            }
            // TODO(plugin): answering 0 here skips what `do_dispatch` does before it reaches the
            // listener loop, and upstream does both regardless of the listener count: the
            // `COMPOSER_DEBUG_EVENTS` trace line, and `push_event`'s circular-call detection
            // (a nested dispatch of the same event name throws there even with no listeners).
            Ok(PluginValue::Int(0))
        }
        other => Err(unavailable_method("EventDispatcher", other)),
    }
}
