//! Serves `CallRustMethod`: the runtime service endpoint (handle 0), the methods every entity
//! answers (construction, clone, property access), and the routing to each class's methods.

use crate::autoload::ClassLoader;
use crate::factory::Factory;
use crate::package::Locker;
use crate::php_worker::console::run_worker_reverse_command;
use crate::php_worker::job;
use crate::php_worker::rpc::args::arg;
use crate::php_worker::rpc::entities::events::dispatch_event_method;
use crate::php_worker::rpc::entity::{Call, Entity};
use crate::php_worker::rpc::table::RustEntity;
use crate::php_worker::rpc::{forwarding, table};
use crate::php_worker::throwable::exception_into_throw;
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpThrow, PluginValue, RustMethodDispatcher, ToWire};
use shirabe_php_shim::{
    AnyThrowable, ToPhpString, bformat, display_to_bytes, error_chain_message, path_to_bytes,
};

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
                    Ok(code) => Ok(code.to_wire()),
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
            .to_wire());
    }
    if method_name == "__shirabe_construct" {
        return construct_entity(args);
    }
    if method_name == "__shirabe_call_static" {
        return call_static_entity(args);
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
            job::settle_php_job(id, Err(PhpThrow::from_wire(description)));
            PluginValue::Null
        }),
        b"__shirabe_cancel_job" => job_id().map(|id| match job::cancel(id) {
            Some(throw) => throw.to_wire(),
            None => PluginValue::Null,
        }),
        b"__shirabe_abort_jobs" => {
            job::abort_internal_jobs(None);
            Ok(PluginValue::Null)
        }
        _ => return None,
    })
}

/// Serves a method call on an entity of a kind still routed through `RustEntity`.
pub(crate) fn dispatch_legacy_entity(
    entity: &RustEntity,
    method_name: &BStr,
    args: &[PluginValue],
    _out_params: &mut IndexMap<u32, PluginValue>,
) -> Result<PluginValue, PhpThrow> {
    if method_name == "__shirabe_clone" {
        return Err(PhpThrow::runtime(
            "cloning this Rust-side entity over RPC is not supported",
        ));
    }
    if matches!(
        method_name.as_bytes(),
        b"__get" | b"__set" | b"__isset" | b"__unset"
    ) {
        let property = arg::<BString>(method_name, args, 0)?;
        // TODO(plugin): the instance properties the proxied classes expose are widened on
        // demand.
        return Err(PhpThrow::runtime(bformat!(
            "the property `{property}` is not available over RPC yet"
        )));
    }
    match entity.clone() {
        RustEntity::Event(event) => {
            dispatch_event_method(&mut *event.borrow_mut(), method_name, args)
        }
    }
}

/// Registers an entity a forwarded constructor built for `class`.
fn constructed<H: Entity>(class: &BStr, entity: H) -> Result<u64, PhpThrow> {
    let (rhandle, crossing) = table::register(entity)?;
    assert_eq!(
        class,
        BStr::new(crossing.name),
        "a constructor builds an entity of its own class"
    );
    Ok(rhandle)
}

/// Serves the constructor every proxy stub carries: plugin code writing `new SomeProxiedClass`
/// gets a fresh Rust-side entity, since the proxied FQCN has no implementation of its own in the
/// child. Classes whose entity cannot be built here are an explicit error, so a plugin never
/// ends up holding a second, unconnected instance of a Composer service.
pub(crate) fn construct_entity(args: &[PluginValue]) -> Result<PluginValue, PhpThrow> {
    let (class, ctor_args) = match (args.first(), args.get(1)) {
        (Some(PluginValue::String(class)), Some(PluginValue::List(ctor_args))) => {
            (class, ctor_args)
        }
        (Some(PluginValue::String(class)), None | Some(PluginValue::Array(_))) => {
            // An empty PHP array crosses as a list; anything else keyed is a protocol error.
            (class, &Vec::new())
        }
        other => {
            return Err(PhpThrow::runtime(bformat!(
                "__shirabe_construct expects a class name and an argument list, got {}",
                display_to_bytes(format_args!("{other:?}"))
            )));
        }
    };
    let class_name = class.as_bstr();
    let mut out_params = IndexMap::new();
    let mut call = Call::new(
        class_name,
        BStr::new("__construct"),
        ctor_args,
        &mut out_params,
    );
    let rhandle = match class.as_slice() {
        b"Composer\\Package\\Package" => constructed(
            class_name,
            <forwarding::package::package::PhpClass as forwarding::package::package::Forwarding>::__construct(&mut call)?,
        )?,
        b"Composer\\Package\\CompletePackage" => constructed(
            class_name,
            <forwarding::package::complete_package::PhpClass as forwarding::package::complete_package::Forwarding>::__construct(&mut call)?,
        )?,
        b"Composer\\Package\\AliasPackage" => constructed(
            class_name,
            <forwarding::package::alias_package::PhpClass as forwarding::package::alias_package::Forwarding>::__construct(&mut call)?,
        )?,
        b"Composer\\Package\\CompleteAliasPackage" => constructed(
            class_name,
            <forwarding::package::complete_alias_package::PhpClass as forwarding::package::complete_alias_package::Forwarding>::__construct(&mut call)?,
        )?,
        b"Composer\\Package\\RootAliasPackage" => constructed(
            class_name,
            <forwarding::package::root_alias_package::PhpClass as forwarding::package::root_alias_package::Forwarding>::__construct(&mut call)?,
        )?,
        b"Composer\\DependencyResolver\\Operation\\InstallOperation" => {
            constructed(
                class_name,
                <forwarding::dependency_resolver::operation::install_operation::PhpClass as forwarding::dependency_resolver::operation::install_operation::Forwarding>::__construct(&mut call)?,
            )?
        }
        b"Composer\\DependencyResolver\\Operation\\UpdateOperation" => {
            constructed(
                class_name,
                <forwarding::dependency_resolver::operation::update_operation::PhpClass as forwarding::dependency_resolver::operation::update_operation::Forwarding>::__construct(&mut call)?,
            )?
        }
        b"Composer\\DependencyResolver\\Operation\\UninstallOperation" => {
            constructed(
                class_name,
                <forwarding::dependency_resolver::operation::uninstall_operation::PhpClass as forwarding::dependency_resolver::operation::uninstall_operation::Forwarding>::__construct(&mut call)?,
            )?
        }
        b"Composer\\DependencyResolver\\Operation\\MarkAliasInstalledOperation" => {
            constructed(
                class_name,
                <forwarding::dependency_resolver::operation::mark_alias_installed_operation::PhpClass as forwarding::dependency_resolver::operation::mark_alias_installed_operation::Forwarding>::__construct(&mut call)?,
            )?
        }
        b"Composer\\DependencyResolver\\Operation\\MarkAliasUninstalledOperation" => {
            constructed(
                class_name,
                <forwarding::dependency_resolver::operation::mark_alias_uninstalled_operation::PhpClass as forwarding::dependency_resolver::operation::mark_alias_uninstalled_operation::Forwarding>::__construct(&mut call)?,
            )?
        }
        b"Composer\\Util\\Filesystem" => {
            constructed(
                class_name,
                <forwarding::util::filesystem::PhpClass as forwarding::util::filesystem::Forwarding>::__construct(&mut call)?,
            )?
        }
        b"Composer\\Util\\ProcessExecutor" => {
            constructed(
                class_name,
                <forwarding::util::process_executor::PhpClass as forwarding::util::process_executor::Forwarding>::__construct(&mut call)?,
            )?
        }
        b"Composer\\Util\\HttpDownloader" => {
            constructed(
                class_name,
                <forwarding::util::http_downloader::PhpClass as forwarding::util::http_downloader::Forwarding>::__construct(&mut call)?,
            )?
        }
        b"Composer\\IO\\BufferIO" => {
            constructed(
                class_name,
                <forwarding::io::buffer_io::PhpClass as forwarding::io::buffer_io::Forwarding>::__construct(&mut call)?,
            )?
        }
        // TODO(plugin): the remaining proxied classes get a construction story on demand. Each
        // one has to decide what a plugin-built instance means for the Rust-side graph, which is
        // why none of them is answered generically here.
        _ => {
            return Err(PhpThrow::runtime(bformat!(
                "Shirabe does not support constructing {class_name} inside the plugin process yet"
            )));
        }
    };
    Ok(PluginValue::Int(rhandle as i64))
}

/// Serves the static methods a proxy stub forwards instead of running locally, because their real
/// bodies reach state the Rust side owns or classes the worker has no code for.
fn call_static_entity(args: &[PluginValue]) -> Result<PluginValue, PhpThrow> {
    let (class, method, call_args) = match (args.first(), args.get(1), args.get(2)) {
        (
            Some(PluginValue::String(class)),
            Some(PluginValue::String(method)),
            Some(PluginValue::List(call_args)),
        ) => (class, method, call_args.clone()),
        (
            Some(PluginValue::String(class)),
            Some(PluginValue::String(method)),
            None | Some(PluginValue::Array(_)),
        ) => (class, method, Vec::new()),
        other => {
            return Err(PhpThrow::runtime(bformat!(
                "__shirabe_call_static expects a class name, a method name and an argument list, got {}",
                display_to_bytes(format_args!("{other:?}"))
            )));
        }
    };
    let name = bformat!("{class}::{method}");
    let string_arg = |position: usize| arg::<BString>(name.as_bstr(), &call_args, position);
    let mut out_params = IndexMap::new();
    let mut call = Call::new(
        class.as_bstr(),
        method.as_bstr(),
        &call_args,
        &mut out_params,
    );
    match (class.as_slice(), method.as_slice()) {
        (b"Composer\\Factory", b"getComposerFile") => Factory::get_composer_file()
            .map(|file| PluginValue::string(path_to_bytes(&file)))
            .map_err(|error| error_throw(b"getComposerFile failed", &error)),
        (b"Composer\\Package\\Locker", b"getContentHash") => {
            Locker::get_content_hash(string_arg(0)?)
                .map(PluginValue::string)
                .map_err(|error| error_throw(b"getContentHash failed", &error))
        }
        (b"Composer\\Util\\ProcessExecutor", b"getTimeout") => {
            <forwarding::util::process_executor::PhpClass as forwarding::util::process_executor::Forwarding>::get_timeout(&mut call)
        }
        (b"Composer\\Util\\ProcessExecutor", b"setTimeout") => {
            <forwarding::util::process_executor::PhpClass as forwarding::util::process_executor::Forwarding>::set_timeout(&mut call)
        }
        _ => Err(PhpThrow::runtime(bformat!(
            "the static method {name} is not available over RPC yet"
        ))),
    }
}

/// The failure of a method of a proxied class the Rust side does not serve. `class` names the
/// class the way the message reads, such as "repository" or "IO".
pub(crate) fn unavailable_method(class: impl AsRef<[u8]>, method: impl AsRef<[u8]>) -> PhpThrow {
    PhpThrow::runtime(bformat!(
        "the {} method `{}` is not available over RPC yet",
        class.as_ref(),
        method.as_ref()
    ))
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
        let throw = construct_entity(&[class, PluginValue::List(Vec::new())]).unwrap_err();
        assert!(
            throw
                .message
                .starts_with(b"Shirabe does not support constructing")
        );
    }
}
