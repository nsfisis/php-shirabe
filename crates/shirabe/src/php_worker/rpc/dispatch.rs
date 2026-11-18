//! Serves the worker's requests: the runtime services a `CallFunction` names, the methods every
//! entity answers (construction, clone, property access), and the routing to each class's
//! methods.

use crate::autoload::ClassLoader;
use crate::php_worker::console::run_worker_reverse_command;
use crate::php_worker::job;
use crate::php_worker::rpc::args::arg;
use crate::php_worker::rpc::entity::Call;
use crate::php_worker::rpc::{forwarding, table};
use crate::php_worker::throwable::exception_into_throw;
use bstr::{BStr, BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpThrow, RpcValue, RustMethodDispatcher, ToRpcValue};
use shirabe_php_shim::{AnyThrowable, bformat, error_chain_message};

/// Looks a class up in every Rust-side `ClassLoader` on the autoload stack, in stack order — the
/// Rust mirror of what the PHP `spl_autoload_register` stack would do in-process.
pub(crate) fn find_file_in_registered_loaders(
    class: impl AsRef<[u8]>,
) -> anyhow::Result<Option<BString>> {
    let class = class.as_ref();
    for loader in ClassLoader::autoload_stack() {
        if let Some(file) = loader.borrow_mut().find_file(class)? {
            return Ok(Some(file));
        }
    }
    Ok(None)
}

/// Serves the requests the worker makes while a call into it is in flight: a `CallFunction` names a
/// runtime service, every other request resolves through the Rust entity table or the forwarding classes.
/// Unsupported methods are explicit errors, never silent fallbacks.
#[derive(Debug)]
pub(crate) struct RpcDispatcher;

impl RustMethodDispatcher for RpcDispatcher {
    fn dispatch(
        &mut self,
        entity_id: u64,
        method_name: &BStr,
        args: Vec<RpcValue>,
        out_params: &mut IndexMap<u32, RpcValue>,
    ) -> Result<RpcValue, PhpThrow> {
        table::call(entity_id, method_name, &args, out_params)
    }

    fn call_function(
        &mut self,
        function_name: &BStr,
        args: Vec<RpcValue>,
    ) -> Result<RpcValue, PhpThrow> {
        call_runtime_service(function_name, &args)
    }

    fn construct(&mut self, class: &BStr, ctor_args: Vec<RpcValue>) -> Result<u64, PhpThrow> {
        construct(class, &ctor_args)
    }

    fn call_static(
        &mut self,
        class: &BStr,
        method_name: &BStr,
        args: Vec<RpcValue>,
    ) -> Result<RpcValue, PhpThrow> {
        call_static(class, method_name, &args)
    }
}

/// Serves the runtime services a `CallFunction` from the worker names: autoload lookups, the
/// built-in commands a plugin-provided command runs, the promise registry, and `abortJobs()`.
fn call_runtime_service(method_name: &BStr, args: &[RpcValue]) -> Result<RpcValue, PhpThrow> {
    if method_name == "__shirabe_run_rust_command" {
        let name = arg::<BString>(method_name, args, 0)?;
        let input_line = arg::<BString>(method_name, args, 1)?;
        return run_worker_reverse_command(name.as_ref(), input_line.as_ref())
            .map(ToRpcValue::to_rpc_value)
            .map_err(|error| error_throw(bformat!("running the command {name} failed"), &error));
    }
    if method_name == "__shirabe_find_file" {
        let class = arg::<BString>(method_name, args, 0)?;
        return Ok(find_file_in_registered_loaders(&class)
            .map_err(|error| error_throw(b"__shirabe_find_file failed", &error))?
            .to_rpc_value());
    }
    if let Some(answer) = dispatch_job_service_method(method_name, args) {
        return answer;
    }
    Err(PhpThrow::runtime(bformat!(
        "unknown runtime service `{method_name}`"
    )))
}

/// Serves the runtime services of the promise registry (`docs/dev/plugin-async.md`) and the
/// `abortJobs()` forward: the worker reports on a promise whenever it settles or is cancelled,
/// whichever call happens to be in flight then. `None` for any other method.
fn dispatch_job_service_method(
    method_name: &BStr,
    args: &[RpcValue],
) -> Option<Result<RpcValue, PhpThrow>> {
    let promise_id = || arg::<i64>(method_name, args, 0).map(|id| id as u64);
    Some(match method_name.as_bytes() {
        b"__shirabe_resolve_promise" => promise_id().map(|id| {
            let value = args.get(1).cloned().unwrap_or(RpcValue::Null);
            job::settle_php_promise(id, Ok(value));
            RpcValue::Null
        }),
        b"__shirabe_reject_promise" => promise_id().map(|id| {
            let record = args
                .get(1)
                .cloned()
                .expect("the worker rejects a promise with the throwable record of its exception");
            job::settle_php_promise(id, Err(PhpThrow::from_rpc_value(record)));
            RpcValue::Null
        }),
        b"__shirabe_cancel_promise" => promise_id().map(|id| match job::cancel(id) {
            Some(throw) => throw.to_rpc_value(),
            None => RpcValue::Null,
        }),
        b"__shirabe_abort_jobs" => {
            job::abort_internal_jobs(None);
            Ok(RpcValue::Null)
        }
        _ => return None,
    })
}

/// Serves a `NewObject` from the worker: plugin code writing `new SomeProxiedClass` gets a fresh
/// Rust-side entity, since the proxied FQCN has no implementation of its own in the child. Classes
/// whose entity cannot be built here are an explicit error, so a plugin never ends up holding a
/// second, unconnected instance of a Composer service.
fn construct(class: &BStr, ctor_args: &[RpcValue]) -> Result<u64, PhpThrow> {
    let mut out_params = IndexMap::new();
    let mut call = Call::new(class, BStr::new("__construct"), ctor_args, &mut out_params);
    forwarding::construct(class, &mut call)
}

/// Serves a `CallStaticMethod` from the worker: the static methods a proxy stub or a guard forwards
/// instead of running locally, because their real bodies reach state the Rust side owns or classes
/// the worker has no code for.
fn call_static(class: &BStr, method_name: &BStr, args: &[RpcValue]) -> Result<RpcValue, PhpThrow> {
    let mut out_params = IndexMap::new();
    let mut call = Call::new(class, method_name, args, &mut out_params);
    forwarding::call_static(class, &mut call)
}

/// The `Throw` a failed Rust-side call crosses the boundary as. An error carrying a ported PHP
/// exception keeps that exception's class, code and declared state, so a plugin catches what it
/// would catch under Composer; one carrying no exception keeps the `RuntimeException` shape,
/// with `context` naming the call that failed.
pub(crate) fn error_throw(context: impl AsRef<[u8]>, error: &anyhow::Error) -> PhpThrow {
    match AnyThrowable::of(error) {
        Some(exception) => exception_into_throw(exception, error),
        None => PhpThrow::runtime(bformat!(
            "{}: {}",
            context.as_ref(),
            error_chain_message(error)
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_function_name_that_is_not_utf8_reaches_the_error_unchanged() {
        let name = b"function\xff";
        let throw = RpcDispatcher
            .call_function(BStr::new(name), Vec::new())
            .unwrap_err();
        assert_eq!(
            BStr::new(b"unknown runtime service `function\xff`"),
            throw.message.as_bstr()
        );
    }

    #[test]
    fn an_argument_of_the_wrong_type_is_a_type_error() {
        let throw = RpcDispatcher
            .call_function(
                BStr::new("__shirabe_run_rust_command"),
                vec![RpcValue::Int(1), RpcValue::string("")],
            )
            .unwrap_err();
        assert_eq!("TypeError", throw.exception_class);
        assert!(
            throw
                .message
                .starts_with(b"__shirabe_run_rust_command argument 0: ")
        );
    }

    #[test]
    fn a_subclass_name_that_is_not_utf8_is_not_constructed() {
        // `static::class` of a plugin's subclass of a proxied class.
        let class = BStr::new(b"Composer\\Package\\Package\xff");
        let throw = RpcDispatcher.construct(class, Vec::new()).unwrap_err();
        assert!(
            throw
                .message
                .starts_with(b"Shirabe does not support constructing")
        );
    }
}
