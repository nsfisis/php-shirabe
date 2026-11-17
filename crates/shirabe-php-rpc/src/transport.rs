use crate::frame::Frame;
use crate::throw::PhpThrow;
use crate::value::{PhpEntityHandle, RpcValue};
use crate::worker::{recv_frame, send_frame};
use crate::{env, session};
use bstr::{BStr, ByteSlice as _};
use indexmap::IndexMap;
use shirabe_php_shim::bformat;

/// Handles the requests the worker sends while a Rust-initiated call is waiting for its `Return`
/// (the cooperative reentrancy loop). A handler may itself issue nested RPC calls.
///
/// A handler whose method takes by-ref parameters writes the value each one holds afterwards into
/// `out_params` under its position, and the stub copies it back into the caller's variable;
/// leaving a position out means the method never assigned to it, which is what PHP does with an
/// untouched by-ref parameter.
pub trait RustMethodDispatcher {
    fn dispatch(
        &mut self,
        entity_id: u64,
        method_name: &BStr,
        args: Vec<RpcValue>,
        out_params: &mut IndexMap<u32, RpcValue>,
    ) -> Result<RpcValue, PhpThrow>;

    /// Runs the runtime service `function_name`: the calls the worker makes that have no receiver
    /// entity.
    fn call_function(
        &mut self,
        function_name: &BStr,
        args: Vec<RpcValue>,
    ) -> Result<RpcValue, PhpThrow>;

    /// Builds a fresh entity of `class` for a proxy stub under construction in the worker, and
    /// answers its Rust entity id.
    fn construct(&mut self, class: &BStr, ctor_args: Vec<RpcValue>) -> Result<u64, PhpThrow>;

    /// Runs a static method of `class` that a proxy stub or a guard in the worker forwards.
    fn call_static(
        &mut self,
        class: &BStr,
        method_name: &BStr,
        args: Vec<RpcValue>,
    ) -> Result<RpcValue, PhpThrow>;
}

/// The Rust side allocates odd correlation ids; the PHP side allocates even ones. Calls nest
/// strictly, so this split is not needed for disambiguation — it just makes any captured frame
/// attributable to its initiator.
static NEXT_CORR_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Allocates a Rust entity id. Ids start at 1, like the worker's PHP entity ids.
static NEXT_ENTITY_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

thread_local! {
    /// Listener for the ReleaseHandle notifications the worker sends, thread-local like the Rust
    /// entity table it prunes.
    static RELEASE_RUST_ENTITY_HOOK: std::cell::RefCell<Option<Box<dyn Fn(u64)>>> =
        const { std::cell::RefCell::new(None) };
}

/// Registers the listener invoked with the released Rust entity id whenever a ReleaseHandle
/// notification arrives on this thread. Replaces any previously registered listener.
pub fn set_release_rust_entity_hook(hook: impl Fn(u64) + 'static) {
    RELEASE_RUST_ENTITY_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(hook));
    });
}

pub fn alloc_entity_id() -> u64 {
    NEXT_ENTITY_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Answers every request from the worker with an explicit error: the dispatcher of calls none of
/// which runs code that calls back.
#[derive(Debug)]
pub struct RefuseCallbacks;

impl RustMethodDispatcher for RefuseCallbacks {
    fn dispatch(
        &mut self,
        entity_id: u64,
        method_name: &BStr,
        _args: Vec<RpcValue>,
        _out_params: &mut IndexMap<u32, RpcValue>,
    ) -> Result<RpcValue, PhpThrow> {
        Err(PhpThrow::runtime(bformat!(
            "no Rust method dispatcher is active for this call (Rust entity id {entity_id}, method `{method_name}`)"
        )))
    }

    fn call_function(
        &mut self,
        function_name: &BStr,
        _args: Vec<RpcValue>,
    ) -> Result<RpcValue, PhpThrow> {
        Err(PhpThrow::runtime(bformat!(
            "no Rust method dispatcher is active for this call (`{function_name}()`)"
        )))
    }

    fn construct(&mut self, class: &BStr, _ctor_args: Vec<RpcValue>) -> Result<u64, PhpThrow> {
        Err(PhpThrow::runtime(bformat!(
            "no Rust method dispatcher is active for this call (new `{class}`)"
        )))
    }

    fn call_static(
        &mut self,
        class: &BStr,
        method_name: &BStr,
        _args: Vec<RpcValue>,
    ) -> Result<RpcValue, PhpThrow> {
        Err(PhpThrow::runtime(bformat!(
            "no Rust method dispatcher is active for this call (`{class}::{method_name}`)"
        )))
    }
}

/// Calls a PHP function in the worker, or the worker's entry point of that name. The outer
/// `Result` is the fatal lane (dead worker, broken framing); the inner one carries a PHP
/// exception if the call threw. `dispatcher` serves the `CallMethod` requests the worker
/// makes while the call is in flight.
pub fn call_function(
    name: impl AsRef<[u8]>,
    args: Vec<RpcValue>,
    dispatcher: &mut dyn RustMethodDispatcher,
) -> anyhow::Result<Result<RpcValue, PhpThrow>> {
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
    args: Vec<RpcValue>,
    dispatcher: &mut dyn RustMethodDispatcher,
) -> anyhow::Result<Result<RpcValue, PhpThrow>> {
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
/// On success the returned value is a `RpcValue::PhpEntityHandle` registered in the worker's P
/// table; the entity stays alive there while the handle does.
pub fn new_object(
    class: impl AsRef<[u8]>,
    ctor_args: Vec<RpcValue>,
    dispatcher: &mut dyn RustMethodDispatcher,
) -> anyhow::Result<Result<RpcValue, PhpThrow>> {
    rpc_call(
        |corr_id| Frame::NewObject {
            corr_id,
            pclass: class.as_ref().into(),
            ctor_args,
        },
        dispatcher,
    )
}

/// Calls `$obj->$method(...$args)` on a PHP entity in the worker.
pub fn call_php_method(
    handle: &PhpEntityHandle,
    method: impl AsRef<[u8]>,
    args: Vec<RpcValue>,
    dispatcher: &mut dyn RustMethodDispatcher,
) -> anyhow::Result<Result<RpcValue, PhpThrow>> {
    rpc_call(
        |corr_id| Frame::CallMethod {
            corr_id,
            entity_id: handle.entity_id(),
            method_name: method.as_ref().into(),
            args,
        },
        dispatcher,
    )
}

/// [`call_function`] for a call that runs no code that calls back.
pub fn call_internal_function(
    name: impl AsRef<[u8]>,
    args: Vec<RpcValue>,
) -> anyhow::Result<Result<RpcValue, PhpThrow>> {
    call_function(name, args, &mut RefuseCallbacks)
}

fn rpc_call(
    request: impl FnOnce(u64) -> Frame,
    dispatcher: &mut dyn RustMethodDispatcher,
) -> anyhow::Result<Result<RpcValue, PhpThrow>> {
    // Held for the whole logical call session; nested calls from the same thread (issued by a
    // dispatcher handler) re-enter immediately, other threads are serialized.
    let session = session::SessionGuard::enter();
    if session.is_outermost() {
        env::flush()?;
    }
    let my_id = NEXT_CORR_ID.fetch_add(2, std::sync::atomic::Ordering::Relaxed);
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
            Frame::CallMethod {
                corr_id,
                entity_id,
                method_name,
                args,
            } => {
                let mut out_params = IndexMap::new();
                let outcome =
                    dispatcher.dispatch(entity_id, method_name.as_bstr(), args, &mut out_params);
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
            Frame::CallFunction {
                corr_id,
                function_name,
                args,
            } => {
                let reply = match dispatcher.call_function(function_name.as_bstr(), args) {
                    Ok(value) => Frame::Return {
                        corr_id,
                        value,
                        out_params: IndexMap::new(),
                    },
                    Err(throw) => Frame::Throw { corr_id, throw },
                };
                send_frame(&reply)?;
            }
            Frame::CallStaticMethod {
                corr_id,
                pclass,
                method_name,
                args,
            } => {
                let reply =
                    match dispatcher.call_static(pclass.as_bstr(), method_name.as_bstr(), args) {
                        Ok(value) => Frame::Return {
                            corr_id,
                            value,
                            out_params: IndexMap::new(),
                        },
                        Err(throw) => Frame::Throw { corr_id, throw },
                    };
                send_frame(&reply)?;
            }
            Frame::NewObject {
                corr_id,
                pclass,
                ctor_args,
            } => {
                let reply = match dispatcher.construct(pclass.as_bstr(), ctor_args) {
                    Ok(entity_id) => Frame::Return {
                        corr_id,
                        value: RpcValue::Int(entity_id as i64),
                        out_params: IndexMap::new(),
                    },
                    Err(throw) => Frame::Throw { corr_id, throw },
                };
                send_frame(&reply)?;
            }
            Frame::ReleaseHandle { entity_id } => {
                // A one-way notification sent by a child-side stub's __destruct; the shirabe crate
                // registers a hook that drops the matching Rust entity table entry. The hook is
                // installed before any entry exists, so an unhooked release is a no-op.
                RELEASE_RUST_ENTITY_HOOK.with(|hook| {
                    if let Some(hook) = hook.borrow().as_ref() {
                        hook(entity_id);
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
