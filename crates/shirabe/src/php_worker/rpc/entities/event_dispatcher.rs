//! The methods a `Composer\EventDispatcher\EventDispatcher` entity answers over RPC.

use crate::event_dispatcher::{Event, EventDispatcherInterface};
use crate::php_worker::call::PhpCall;
use crate::php_worker::rpc::entity::Call;
use bstr::{BStr, BString};
use shirabe_php_binding_macros::php_entity;
use shirabe_php_rpc::{PhpThrow, RpcValue};
use shirabe_php_shim::bformat;

/// PHP's `is_callable($value)`, answered by the worker because only its own function and class
/// tables can say whether a string or a `[$object, 'method']` pair names something callable.
/// Values that cannot name a callable at all are answered here rather than over a round trip.
pub(crate) fn is_php_callable(value: &RpcValue) -> anyhow::Result<bool> {
    match value {
        RpcValue::String(_)
        | RpcValue::List(_)
        | RpcValue::Array(_)
        | RpcValue::PhpEntityHandle(_) => {
            PhpCall::Function(BStr::new("is_callable")).run(vec![value.clone()])
        }
        _ => Ok(false),
    }
}

/// Calls a PHP callable value in the worker, whatever form it takes: `call_user_func` resolves a
/// closure, a function name and an `[$object, 'method']` pair exactly as the original call site
/// would have.
pub(crate) fn call_php_callable(
    callable: &RpcValue,
    args: Vec<RpcValue>,
) -> anyhow::Result<RpcValue> {
    let mut call_args = Vec::with_capacity(args.len() + 1);
    call_args.push(callable.clone());
    call_args.extend(args);
    PhpCall::Function(BStr::new("call_user_func")).run(call_args)
}

php_entity! {
    handle: std::rc::Rc<std::cell::RefCell<dyn EventDispatcherInterface>>,
    expected: "an EventDispatcher",
    class: r"Composer\EventDispatcher\EventDispatcher",
    types: {
        r"Composer\EventDispatcher\EventDispatcher": custom self,
    },
}

fn dispatch(
    dispatcher: &std::rc::Rc<std::cell::RefCell<dyn EventDispatcherInterface>>,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let name = call.arg::<BString>(0)?;
    let probe = Event::from_name(name.clone());
    if dispatcher.borrow_mut().has_event_listeners(&probe) {
        // TODO(plugin): dispatching a worker-constructed event through the Rust-side
        // dispatcher needs the event object (and the console input it carries) proxied back
        // into this process, so only the no-listener case is answered here.
        return Err(PhpThrow::runtime(bformat!(
            "dispatching `{name}` from the plugin process is not supported yet while listeners are registered for it"
        )));
    }
    // TODO(plugin): answering 0 here skips what `do_dispatch` does before it reaches the
    // listener loop, and upstream does both regardless of the listener count: the
    // `COMPOSER_DEBUG_EVENTS` trace line, and `push_event`'s circular-call detection (a
    // nested dispatch of the same event name throws there even with no listeners).
    Ok(RpcValue::Int(0))
}
