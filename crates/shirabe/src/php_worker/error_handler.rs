//! `Composer\Util\ErrorHandler` runs in the PHP worker, the only place where PHP errors are raised
//! (by plugin and script code). [`register`] pushes `ErrorHandler::register($io)` there.
//!
//! The worker can autoload the class only once the Composer PHP runtime is loaded there, and no
//! plugin or script code can run before that either. Until then, the calls to [`register`] and
//! [`restore_error_handler`] are kept as a stack of the handlers they would have installed, which
//! is replayed in the worker right after the runtime is loaded (see [`register_pending`]).

use crate::io::IOInterface;
use crate::php_worker::call::PhpCall;
use crate::php_worker::rpc::entity::entity_value;
use crate::php_worker::throwable::throw_into_error;
use bstr::BStr;
use shirabe_php_rpc::RpcValue;

#[derive(Debug)]
enum State {
    /// The Composer PHP runtime is not loaded into the worker yet. Holds the `$io` of each
    /// handler registered so far, innermost last.
    Pending(Vec<Option<std::rc::Rc<dyn IOInterface>>>),
    /// The runtime is loaded, and every call goes to the worker as it is made.
    Forwarding,
}

thread_local! {
    static STATE: std::cell::RefCell<State> = const { std::cell::RefCell::new(State::Pending(Vec::new())) };
}

/// `ErrorHandler::register($io)`.
pub(crate) fn register(io: Option<std::rc::Rc<dyn IOInterface>>) -> anyhow::Result<()> {
    let io = STATE.with(|state| match &mut *state.borrow_mut() {
        State::Pending(stack) => {
            stack.push(io);
            None
        }
        State::Forwarding => Some(io),
    });
    match io {
        Some(io) => push_io(io),
        None => Ok(()),
    }
}

/// `restore_error_handler()`.
pub fn restore_error_handler() -> anyhow::Result<()> {
    let forwarding = STATE.with(|state| match &mut *state.borrow_mut() {
        State::Pending(stack) => {
            stack.pop();
            false
        }
        State::Forwarding => true,
    });
    if forwarding {
        PhpCall::Function(BStr::new("restore_error_handler")).run::<RpcValue>(Vec::new())?;
    }
    Ok(())
}

/// Replays the handlers registered before the Composer PHP runtime was loaded into the worker.
/// Called right after it is loaded there, before any plugin or script code runs.
pub(crate) fn register_pending() -> anyhow::Result<()> {
    let pending =
        STATE.with(
            |state| match std::mem::replace(&mut *state.borrow_mut(), State::Forwarding) {
                State::Pending(stack) => stack,
                State::Forwarding => Vec::new(),
            },
        );
    for io in pending {
        push_io(io)?;
    }
    Ok(())
}

/// Pushes `ErrorHandler::$io` into the worker through `ErrorHandler::register($io)`.
pub(crate) fn push_io(io: Option<std::rc::Rc<dyn IOInterface>>) -> anyhow::Result<()> {
    let io = match io {
        Some(io) => entity_value(&io).map_err(throw_into_error)?,
        None => RpcValue::Null,
    };
    PhpCall::StaticMethod(
        BStr::new("Composer\\Util\\ErrorHandler"),
        BStr::new("register"),
    )
    .run::<RpcValue>(vec![io])?;
    Ok(())
}
