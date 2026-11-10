//! Serves `CallRustMethod`: the runtime service endpoint (handle 0), the methods every entity
//! answers (construction, clone, property access), and the routing to each class's methods.

use crate::autoload::ClassLoader;
use crate::php_worker::console::run_worker_reverse_command;
use crate::php_worker::job;
use crate::php_worker::rpc::args::arg;
use crate::php_worker::rpc::entity::Call;
use crate::php_worker::rpc::{forwarding, table};
use crate::php_worker::throwable::exception_into_throw;
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpThrow, PluginValue, RustMethodDispatcher, ToPluginValue};
use shirabe_php_shim::{AnyThrowable, ToPhpString, bformat, error_chain_message};

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

/// Serves `CallRustMethod` requests from the child while a plugin-related call is in flight:
/// rhandle 0 is the runtime service endpoint (autoload lookups, the built-in commands a
/// plugin-provided command runs), every other rhandle resolves through the R table. Unsupported
/// methods are explicit errors, never silent fallbacks.
#[derive(Debug)]
pub(crate) struct PluginRpcDispatcher;

impl RustMethodDispatcher for PluginRpcDispatcher {
    fn dispatch(
        &mut self,
        rhandle: u64,
        method_name: &BStr,
        args: Vec<PluginValue>,
        out_params: &mut IndexMap<u32, PluginValue>,
    ) -> Result<PluginValue, PhpThrow> {
        if rhandle == 0 {
            if method_name == "__shirabe_run_rust_command" {
                let name = arg::<BString>(method_name, &args, 0)?;
                let input_line = arg::<BString>(method_name, &args, 1)?;
                return match run_worker_reverse_command(name.as_ref(), input_line.as_ref()) {
                    Ok(code) => Ok(code.to_plugin_value()),
                    Err(e) => Err(PhpThrow::runtime(error_chain_message(&e).to_php_string())),
                };
            }
            return dispatch_runtime_service_method(method_name, &args);
        }

        table::call(rhandle, method_name, &args, out_params)
    }
}

/// Serves `CallRustMethod` requests issued by the PHP worker while a script-related call is in
/// flight: Rust handle 0 is the runtime service endpoint (autoload lookups against the
/// Rust-side [`ClassLoader`]s on the autoload stack), every other rhandle resolves through the R
/// table.
#[derive(Debug)]
pub(crate) struct ScriptRpcDispatcher;

impl RustMethodDispatcher for ScriptRpcDispatcher {
    fn dispatch(
        &mut self,
        rhandle: u64,
        method_name: &BStr,
        args: Vec<PluginValue>,
        out_params: &mut IndexMap<u32, PluginValue>,
    ) -> Result<PluginValue, PhpThrow> {
        if rhandle == 0 {
            return dispatch_runtime_service_method(method_name, &args);
        }
        table::call(rhandle, method_name, &args, out_params)
    }
}

/// Serves the runtime service methods (Rust handle 0) every dispatcher answers: autoload
/// lookups, entity construction and static calls, and the job registry.
fn dispatch_runtime_service_method(
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    if method_name == "__shirabe_find_file" {
        let class = arg::<BString>(method_name, args, 0)?;
        return Ok(find_file_in_registered_loaders(&class)
            .map_err(|error| error_throw(b"__shirabe_find_file failed", &error))?
            .to_plugin_value());
    }
    if method_name == "__shirabe_construct" {
        return construct(args);
    }
    if method_name == "__shirabe_call_static" {
        return call_static(args);
    }
    if let Some(answer) = dispatch_job_service_method(method_name, args) {
        return answer;
    }
    Err(PhpThrow::runtime(bformat!(
        "unknown runtime service method `{method_name}`"
    )))
}

/// Serves the runtime service methods of the job registry (`docs/dev/plugin-async.md`), shared by
/// every dispatcher: the worker reports on a job whenever the promise behind it settles or is
/// cancelled, whichever call happens to be in flight then. `None` for any other method.
fn dispatch_job_service_method(
    method_name: &BStr,
    args: &[PluginValue],
) -> Option<Result<PluginValue, PhpThrow>> {
    let job_id = || arg::<i64>(method_name, args, 0).map(|id| id as u64);
    Some(match method_name.as_bytes() {
        b"__shirabe_resolve_job" => job_id().map(|id| {
            let value = args.get(1).cloned().unwrap_or(PluginValue::Null);
            job::settle_php_job(id, Ok(value));
            PluginValue::Null
        }),
        b"__shirabe_reject_job" => job_id().map(|id| {
            let description = args
                .get(1)
                .cloned()
                .expect("the worker rejects a job with the description of its exception");
            job::settle_php_job(id, Err(PhpThrow::from_plugin_value(description)));
            PluginValue::Null
        }),
        b"__shirabe_cancel_job" => job_id().map(|id| match job::cancel(id) {
            Some(throw) => throw.to_plugin_value(),
            None => PluginValue::Null,
        }),
        b"__shirabe_abort_jobs" => {
            job::abort_internal_jobs(None);
            Ok(PluginValue::Null)
        }
        _ => return None,
    })
}

/// Serves `__shirabe_construct`: plugin code writing `new SomeProxiedClass` gets a fresh Rust-side
/// entity, since the proxied FQCN has no implementation of its own in the child. Classes whose
/// entity cannot be built here are an explicit error, so a plugin never ends up holding a second,
/// unconnected instance of a Composer service.
fn construct(args: &[PluginValue]) -> Result<PluginValue, PhpThrow> {
    let (class, ctor_args) = match (args.first(), args.get(1)) {
        (Some(PluginValue::String(class)), Some(PluginValue::List(ctor_args))) => {
            (class, ctor_args.as_slice())
        }
        (Some(PluginValue::String(class)), None | Some(PluginValue::Array(_))) => {
            (class, [].as_slice())
        }
        other => panic!(
            "protocol violation: __shirabe_construct expects a class name and an argument list, got {other:?}"
        ),
    };
    let mut out_params = IndexMap::new();
    let mut call = Call::new(
        class.as_bstr(),
        BStr::new("__construct"),
        ctor_args,
        &mut out_params,
    );
    let rhandle = forwarding::construct(class.as_bstr(), &mut call)?;
    Ok(PluginValue::Int(rhandle as i64))
}

/// Serves `__shirabe_call_static`: the static methods a proxy stub forwards instead of running
/// locally, because their real bodies reach state the Rust side owns or classes the worker has no
/// code for.
fn call_static(args: &[PluginValue]) -> Result<PluginValue, PhpThrow> {
    let (class, method, call_args) = match (args.first(), args.get(1), args.get(2)) {
        (
            Some(PluginValue::String(class)),
            Some(PluginValue::String(method)),
            Some(PluginValue::List(call_args)),
        ) => (class, method, call_args.as_slice()),
        (
            Some(PluginValue::String(class)),
            Some(PluginValue::String(method)),
            None | Some(PluginValue::Array(_)),
        ) => (class, method, [].as_slice()),
        other => panic!(
            "protocol violation: __shirabe_call_static expects a class name, a method name and an argument list, got {other:?}"
        ),
    };
    let mut out_params = IndexMap::new();
    let mut call = Call::new(
        class.as_bstr(),
        method.as_bstr(),
        call_args,
        &mut out_params,
    );
    forwarding::call_static(class.as_bstr(), &mut call)
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
    fn a_method_name_that_is_not_utf8_reaches_the_error_unchanged() {
        let name = b"method\xff";
        let throw = PluginRpcDispatcher
            .dispatch(0, BStr::new(name), Vec::new(), &mut IndexMap::new())
            .unwrap_err();
        assert_eq!(
            BStr::new(b"unknown runtime service method `method\xff`"),
            throw.message.as_bstr()
        );
    }

    #[test]
    fn a_subclass_name_that_is_not_utf8_is_not_constructed() {
        // `static::class` of a plugin's subclass of a proxied class.
        let class =
            PluginValue::String(BString::from(b"Composer\\Package\\Package\xff".as_slice()));
        let throw = construct(&[class, PluginValue::List(Vec::new())]).unwrap_err();
        assert!(
            throw
                .message
                .starts_with(b"Shirabe does not support constructing")
        );
    }
}
