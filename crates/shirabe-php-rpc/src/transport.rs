use crate::frame::Frame;
use crate::throw::PhpThrow;
use crate::value::{PhpObjHandle, PluginValue};
use crate::worker::{recv_frame, send_frame};
use crate::{env, session};
use bstr::{BStr, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::bformat;
use std::sync::atomic::{AtomicU64, Ordering};

/// Handles `CallRustMethod` requests arriving while a Rust-initiated call is waiting for its
/// `Return` (the cooperative reentrancy loop). A handler may itself issue nested RPC calls.
///
/// A handler whose method takes by-ref parameters writes the value each one holds afterwards into
/// `out_params` under its position, and the stub copies it back into the caller's variable;
/// leaving a position out means the method never assigned to it, which is what PHP does with an
/// untouched by-ref parameter.
pub trait RustMethodDispatcher {
    fn dispatch(
        &mut self,
        rhandle: u64,
        method_name: &BStr,
        args: Vec<PluginValue>,
        out_params: &mut IndexMap<u32, PluginValue>,
    ) -> Result<PluginValue, PhpThrow>;
}

/// The Rust side allocates odd correlation ids; the PHP side allocates even ones. Calls nest
/// strictly, so this split is not needed for disambiguation — it just makes any captured frame
/// attributable to its initiator.
static NEXT_CORR_ID: AtomicU64 = AtomicU64::new(1);

/// Allocates a Rust-side object handle. Handle 0 is reserved for the runtime service endpoint
/// (e.g. `__shirabe_find_file` autoload queries), so ids start at 1.
static NEXT_RHANDLE: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// Listener for ReleaseRustHandle notifications, thread-local like the R table it prunes.
    static RELEASE_RUST_HANDLE_HOOK: std::cell::RefCell<Option<Box<dyn Fn(u64)>>> =
        const { std::cell::RefCell::new(None) };
}

/// Registers the listener invoked with the released handle whenever a ReleaseRustHandle
/// notification arrives on this thread. Replaces any previously registered listener.
pub fn set_release_rust_handle_hook(hook: impl Fn(u64) + 'static) {
    RELEASE_RUST_HANDLE_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(hook));
    });
}

pub fn alloc_rhandle() -> u64 {
    NEXT_RHANDLE.fetch_add(1, Ordering::Relaxed)
}

/// Answers every `CallRustMethod` with an explicit error: the dispatcher of the calls this crate
/// makes for itself, none of which runs code that calls back.
#[derive(Debug)]
pub(crate) struct RefuseCallbacks;

impl RustMethodDispatcher for RefuseCallbacks {
    fn dispatch(
        &mut self,
        rhandle: u64,
        method_name: &BStr,
        _args: Vec<PluginValue>,
        _out_params: &mut IndexMap<u32, PluginValue>,
    ) -> Result<PluginValue, PhpThrow> {
        Err(PhpThrow::runtime(bformat!(
            "no Rust method dispatcher is active for this call (rhandle {rhandle}, method `{method_name}`)"
        )))
    }
}

/// Calls a PHP function in the worker, or the worker's entry point of that name. The outer
/// `Result` is the fatal lane (dead worker, broken framing); the inner one carries a PHP
/// exception if the call threw. `dispatcher` serves the `CallRustMethod` requests the worker
/// makes while the call is in flight.
pub fn call_function(
    name: impl AsRef<[u8]>,
    args: Vec<PluginValue>,
    dispatcher: &mut dyn RustMethodDispatcher,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    rpc_call(
        |corr_id| Frame::CallFunction {
            corr_id,
            function_name: name.as_ref().into(),
            args,
        },
        dispatcher,
    )
}

/// Calls `$class::$method(...$args)` in the worker (autoloading the class if needed).
pub fn call_static_method(
    class: impl AsRef<[u8]>,
    method: impl AsRef<[u8]>,
    args: Vec<PluginValue>,
    dispatcher: &mut dyn RustMethodDispatcher,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    rpc_call(
        |corr_id| Frame::CallStaticMethod {
            corr_id,
            pclass: class.as_ref().into(),
            method_name: method.as_ref().into(),
            args,
        },
        dispatcher,
    )
}

/// Instantiates `new $class(...$ctor_args)` in the worker (autoloading the class if needed).
/// On success the returned value is a `PluginValue::PhpHandle` registered in the worker's P
/// table; the entity stays alive there while the handle does.
pub fn new_object(
    class: impl AsRef<[u8]>,
    ctor_args: Vec<PluginValue>,
    dispatcher: &mut dyn RustMethodDispatcher,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    rpc_call(
        |corr_id| Frame::NewObject {
            corr_id,
            pclass: class.as_ref().into(),
            ctor_args,
        },
        dispatcher,
    )
}

/// Calls `$obj->$method(...$args)` on a P-table entity in the worker.
pub fn call_php_method(
    handle: &PhpObjHandle,
    method: impl AsRef<[u8]>,
    args: Vec<PluginValue>,
    dispatcher: &mut dyn RustMethodDispatcher,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    rpc_call(
        |corr_id| Frame::CallPhpMethod {
            corr_id,
            phandle: handle.phandle(),
            method_name: method.as_ref().into(),
            args,
        },
        dispatcher,
    )
}

/// [`call_function`] for the calls this crate makes for itself.
pub(crate) fn call_internal_function(
    name: impl AsRef<[u8]>,
    args: Vec<PluginValue>,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    call_function(name, args, &mut RefuseCallbacks)
}

fn rpc_call(
    request: impl FnOnce(u64) -> Frame,
    dispatcher: &mut dyn RustMethodDispatcher,
) -> anyhow::Result<Result<PluginValue, PhpThrow>> {
    // Held for the whole logical call session; nested calls from the same thread (issued by a
    // dispatcher handler) re-enter immediately, other threads are serialized.
    let session = session::SessionGuard::enter();
    if session.is_outermost() {
        env::flush()?;
    }
    let my_id = NEXT_CORR_ID.fetch_add(2, Ordering::Relaxed);
    send_frame(&request(my_id))?;
    loop {
        let incoming = recv_frame()?;
        match incoming {
            Frame::Return {
                corr_id,
                value,
                out_params,
            } if corr_id == my_id => {
                assert!(
                    out_params.is_empty(),
                    "PHP RPC: protocol violation — the worker answered with out params: \
                     {out_params:?}"
                );
                return Ok(Ok(value));
            }
            Frame::Throw { corr_id, throw } if corr_id == my_id => {
                return Ok(Err(throw));
            }
            Frame::CallRustMethod {
                corr_id,
                rhandle,
                method_name,
                args,
            } => {
                let mut out_params = IndexMap::new();
                let outcome =
                    dispatcher.dispatch(rhandle, method_name.as_bstr(), args, &mut out_params);
                let reply = match outcome {
                    Ok(value) => Frame::Return {
                        corr_id,
                        value,
                        out_params,
                    },
                    Err(throw) => Frame::Throw { corr_id, throw },
                };
                send_frame(&reply)?;
            }
            Frame::ReleaseRustHandle { rhandle } => {
                // A one-way notification sent by a child-side stub's __destruct; the shirabe
                // crate registers a hook that drops the matching R-table entry. The hook is
                // installed before any entry exists, so an unhooked release is a no-op.
                RELEASE_RUST_HANDLE_HOOK.with(|hook| {
                    if let Some(hook) = hook.borrow().as_ref() {
                        hook(rhandle);
                    }
                });
                continue;
            }
            other => panic!(
                "PHP RPC: protocol violation — unexpected frame while waiting for corr_id \
                 {my_id}: {other:?}"
            ),
        }
    }
}
