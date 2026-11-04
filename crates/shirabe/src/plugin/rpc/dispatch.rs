//! Serves `CallRustMethod`: the runtime service endpoint (handle 0), the methods every entity
//! answers (construction, clone, property access), and the routing to each class's methods.

use crate::autoload::ClassLoader;
use crate::dependency_resolver::operation::AnyOperation;
use crate::event_dispatcher::EventInterface;
use crate::event_dispatcher::event_dispatcher::dispatch_event_method;
use crate::io::IOInterface;
use crate::package::PackageInterfaceHandle;
use crate::package::handle::AnyPackage;
use crate::plugin::job_registry;
use crate::plugin::rpc::codec::{arg, arg_or};
use crate::plugin::rpc::entities::autoload_generator::dispatch_autoload_generator_method;
use crate::plugin::rpc::entities::composer::dispatch_composer_method;
use crate::plugin::rpc::entities::config::dispatch_config_method;
use crate::plugin::rpc::entities::download_manager::dispatch_download_manager_method;
use crate::plugin::rpc::entities::event_dispatcher::dispatch_event_dispatcher_method;
use crate::plugin::rpc::entities::filesystem::dispatch_filesystem_method;
use crate::plugin::rpc::entities::http_downloader::dispatch_http_downloader_method;
use crate::plugin::rpc::entities::installation_manager::dispatch_installation_manager_method;
use crate::plugin::rpc::entities::io::dispatch_io_method;
use crate::plugin::rpc::entities::locker::dispatch_locker_method;
use crate::plugin::rpc::entities::operation::dispatch_operation_method;
use crate::plugin::rpc::entities::package::dispatch_package_method;
use crate::plugin::rpc::entities::plugin::dispatch_plugin_method;
use crate::plugin::rpc::entities::process::dispatch_process_method;
use crate::plugin::rpc::entities::process_executor::dispatch_process_executor_method;
use crate::plugin::rpc::entities::repository::dispatch_repository_method;
use crate::plugin::rpc::entities::repository_manager::dispatch_repository_manager_method;
use crate::plugin::rpc::entities::transaction::dispatch_transaction_method;
use crate::plugin::rpc::table::{R_TABLE, RustEntity, entity_arg, register_entity};
use crate::plugin::throwable_bridge::exception_into_throw;
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpThrow, PluginValue, RustMethodDispatcher, ToWire};
use shirabe_php_shim::{
    AnyThrowable, PhpMixed, bconcat, display_to_bytes, error_chain_message, path_to_bytes,
};

/// Looks a class up in every registered Rust-side `ClassLoader`, in registration order — the
/// Rust mirror of what the PHP `spl_autoload_register` stack would do in-process.
///
/// TODO(plugin): `ClassLoader::register` keeps one loader per vendor-dir (matching upstream's
/// `$registeredLoaders`), but the real spl stack keeps every registered loader; because the
/// registered loaders are the only autoload stack here, registering a second plugin loader under
/// the same vendor-dir evicts the first one, and a class of the earlier plugin that was never
/// loaded can become unresolvable (PHP would still find it).
pub(crate) fn find_file_in_registered_loaders(
    class: impl AsRef<[u8]>,
) -> anyhow::Result<Option<BString>> {
    let class = class.as_ref();
    for (_vendor_dir, mut loader) in ClassLoader::get_registered_loaders() {
        if let Some(file) = loader.find_file(class)? {
            return Ok(Some(file));
        }
    }
    Ok(None)
}

/// Serves `CallRustMethod` requests from the child while a plugin-related call is in flight:
/// rhandle 0 is the runtime service endpoint (autoload lookups), every other rhandle resolves
/// through the R table. Unsupported methods are explicit errors, never silent fallbacks.
#[derive(Debug, Default)]
pub(crate) struct PluginRpcDispatcher<'a> {
    /// At most one live event handle exposed per dispatched listener call, alongside the
    /// persistent R table.
    ///
    /// TODO(plugin): a listener that stores the event stub beyond its own call observes an
    /// unknown handle error afterwards; keeping events in the R table needs full proxying of
    /// the object graph an event exposes, which does not exist yet.
    pub(crate) event: Option<(u64, &'a mut dyn EventInterface)>,
}

impl RustMethodDispatcher for PluginRpcDispatcher<'_> {
    fn dispatch(
        &mut self,
        rhandle: u64,
        method_name: &BStr,
        args: Vec<PluginValue>,
        out_params: &mut IndexMap<u32, PluginValue>,
    ) -> Result<PluginValue, PhpThrow> {
        if rhandle == 0 {
            if method_name == "__shirabe_find_file" {
                let class = arg::<BString>(method_name, &args, 0)?;
                return Ok(find_file_in_registered_loaders(&class)
                    .map_err(|error| error_throw(b"__shirabe_find_file failed", &error))?
                    .to_wire());
            }
            if method_name == "__shirabe_run_rust_command" {
                let name = arg::<BString>(method_name, &args, 0)?;
                let input_line = arg::<BString>(method_name, &args, 1)?;
                return match crate::console::application::run_worker_reverse_command(
                    name.as_ref(),
                    input_line.as_ref(),
                ) {
                    Ok(code) => Ok(code.to_wire()),
                    Err(e) => Err(PhpThrow::runtime(bconcat!(error_chain_message(&e)))),
                };
            }
            if method_name == "__shirabe_construct" {
                return construct_entity(&args);
            }
            if method_name == "__shirabe_call_static" {
                return call_static_entity(&args);
            }
            if let Some(answer) = dispatch_job_service_method(method_name, &args) {
                return answer;
            }
            return Err(PhpThrow::runtime(bconcat!(
                "unknown runtime service method `",
                method_name,
                "`"
            )));
        }

        if let Some((event_rhandle, ref mut event)) = self.event
            && event_rhandle == rhandle
        {
            return dispatch_event_method(&mut **event, method_name, &args);
        }

        dispatch_r_table_method(rhandle, method_name, &args, out_params)
    }
}

/// Serves the runtime service methods of the job registry (`docs/dev/plugin-async.md`), shared by
/// every dispatcher: the worker reports on a job whenever the promise behind it settles or is
/// cancelled, whichever call happens to be in flight then. `None` for any other method.
pub(crate) fn dispatch_job_service_method(
    method_name: &BStr,
    args: &[PluginValue],
) -> Option<Result<PluginValue, PhpThrow>> {
    let job_id = || arg::<i64>(method_name, args, 0).map(|id| id as u64);
    Some(match method_name.as_bytes() {
        b"__shirabe_resolve_job" => job_id().map(|id| {
            let value = args.get(1).cloned().unwrap_or(PluginValue::Null);
            job_registry::settle_php_job(id, Ok(value));
            PluginValue::Null
        }),
        b"__shirabe_reject_job" => job_id().map(|id| {
            let description = args
                .get(1)
                .cloned()
                .expect("the worker rejects a job with the description of its exception");
            job_registry::settle_php_job(id, Err(PhpThrow::from_wire(description)));
            PluginValue::Null
        }),
        b"__shirabe_cancel_job" => job_id().map(|id| match job_registry::cancel(id) {
            Some(throw) => throw.to_wire(),
            None => PluginValue::Null,
        }),
        b"__shirabe_abort_jobs" => {
            job_registry::abort_internal_jobs(None);
            Ok(PluginValue::Null)
        }
        _ => return None,
    })
}

/// Serves a method call on an R-table entity, shared by every dispatcher: the child holds a
/// stub for an entity registered by an earlier call, and the handle outlives the call that
/// minted it.
pub(crate) fn dispatch_r_table_method(
    rhandle: u64,
    method_name: &BStr,
    args: &[PluginValue],
    out_params: &mut IndexMap<u32, PluginValue>,
) -> Result<PluginValue, PhpThrow> {
    let entity = R_TABLE.with(|table| table.borrow().get(&rhandle).cloned());
    if method_name == "__shirabe_clone" {
        return match entity {
            Some(entity) => clone_entity(&entity),
            None => Err(PhpThrow::runtime(bconcat!("unknown Rust handle ", rhandle))),
        };
    }
    if matches!(
        method_name.as_bytes(),
        b"__get" | b"__set" | b"__isset" | b"__unset"
    ) {
        return match entity {
            Some(entity) => dispatch_property_access(&entity, method_name, args),
            None => Err(PhpThrow::runtime(bconcat!("unknown Rust handle ", rhandle))),
        };
    }
    match entity {
        Some(RustEntity::Io(io)) => dispatch_io_method(&io, method_name, args),
        Some(RustEntity::Composer(composer)) => dispatch_composer_method(&composer, method_name),
        Some(RustEntity::AutoloadGenerator(generator)) => {
            dispatch_autoload_generator_method(&generator, method_name, args)
        }
        Some(RustEntity::Config(config)) => dispatch_config_method(&config, method_name, args),
        Some(RustEntity::DownloadManager(dm)) => {
            dispatch_download_manager_method(&dm, method_name, args)
        }
        Some(RustEntity::Filesystem(fs)) => dispatch_filesystem_method(&fs, method_name, args),
        Some(RustEntity::InstallationManager(im)) => {
            dispatch_installation_manager_method(&im, method_name, args)
        }
        Some(RustEntity::Locker(locker)) => dispatch_locker_method(&locker, method_name, args),
        Some(RustEntity::RepositoryManager(rm)) => {
            dispatch_repository_manager_method(&rm, method_name)
        }
        Some(RustEntity::Repository(repository)) => {
            dispatch_repository_method(&repository, method_name, args)
        }
        Some(RustEntity::Package(package)) => dispatch_package_method(&package, method_name, args),
        Some(RustEntity::EventDispatcher(dispatcher)) => {
            dispatch_event_dispatcher_method(&dispatcher, method_name, args)
        }
        Some(RustEntity::Operation(operation)) => {
            dispatch_operation_method(&operation, method_name, args)
        }
        Some(RustEntity::Transaction(transaction)) => {
            dispatch_transaction_method(&transaction, method_name)
        }
        Some(RustEntity::Plugin(plugin)) => dispatch_plugin_method(&plugin, method_name),
        Some(RustEntity::ProcessExecutor(process)) => {
            dispatch_process_executor_method(&process, method_name, args, out_params)
        }
        Some(RustEntity::HttpDownloader(downloader)) => {
            dispatch_http_downloader_method(&downloader, method_name, args)
        }
        Some(RustEntity::Process(process)) => dispatch_process_method(&process, method_name),
        None => Err(PhpThrow::runtime(bconcat!("unknown Rust handle ", rhandle))),
    }
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
            return Err(PhpThrow::runtime(bconcat!(
                "__shirabe_construct expects a class name and an argument list, got ",
                display_to_bytes(format_args!("{other:?}"))
            )));
        }
    };
    let class_name = class.as_bstr();
    let string_arg = |position: usize| arg::<BString>(class_name, ctor_args, position);
    let package_arg =
        |position: usize| arg::<PackageInterfaceHandle>(class_name, ctor_args, position);
    let io_arg = |position: usize| {
        entity_arg::<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>(
            class_name, ctor_args, position,
        )
    };
    let process_executor_arg = |position: usize| {
        entity_arg::<std::rc::Rc<std::cell::RefCell<crate::util::ProcessExecutor>>>(
            class_name, ctor_args, position,
        )
    };
    let config_arg = |position: usize| {
        entity_arg::<std::rc::Rc<std::cell::RefCell<crate::config::Config>>>(
            class_name, ctor_args, position,
        )
    };
    let alias_package_arg =
        |position: usize| -> Result<crate::package::AliasPackageHandle, PhpThrow> {
            package_arg(position)?
                .as_alias()
                .ok_or_else(|| PhpThrow::runtime(bconcat!(class_name, " expects an AliasPackage")))
        };
    let package = match class.as_slice() {
        b"Composer\\Package\\Package" => AnyPackage::Package(crate::package::Package::new(
            string_arg(0)?,
            string_arg(1)?,
            string_arg(2)?,
        )),
        b"Composer\\Package\\CompletePackage" => AnyPackage::CompletePackage(
            crate::package::CompletePackage::new(string_arg(0)?, string_arg(1)?, string_arg(2)?),
        ),
        // The alias target has to be a package that already lives on the Rust side; an alias of
        // an alias has no Rust representation, so its narrowing is an explicit error too.
        b"Composer\\Package\\AliasPackage" => {
            let alias_of = package_arg(0)?.as_package().ok_or_else(|| {
                PhpThrow::runtime(bconcat!(class_name, " expects a real Package to alias"))
            })?;
            AnyPackage::AliasPackage(crate::package::AliasPackage::new(
                alias_of,
                string_arg(1)?,
                string_arg(2)?,
            ))
        }
        b"Composer\\Package\\CompleteAliasPackage" => {
            let alias_of = package_arg(0)?.as_complete_package().ok_or_else(|| {
                PhpThrow::runtime(bconcat!(
                    class_name,
                    " expects a real CompletePackage to alias"
                ))
            })?;
            AnyPackage::CompleteAliasPackage(crate::package::CompleteAliasPackage::new(
                alias_of,
                string_arg(1)?,
                string_arg(2)?,
            ))
        }
        b"Composer\\Package\\RootAliasPackage" => {
            let alias_of = package_arg(0)?.as_root_package().ok_or_else(|| {
                PhpThrow::runtime(bconcat!(class_name, " expects a real RootPackage to alias"))
            })?;
            AnyPackage::RootAliasPackage(crate::package::RootAliasPackage::new(
                alias_of,
                string_arg(1)?,
                string_arg(2)?,
            ))
        }
        // A solver operation carries no state beyond the packages it names, so a plugin-built
        // one is a complete instance rather than a second view on a Rust-side service.
        b"Composer\\DependencyResolver\\Operation\\InstallOperation" => {
            return Ok(operation_construction_result(AnyOperation::Install(
                crate::dependency_resolver::operation::InstallOperation::new(package_arg(0)?),
            )));
        }
        b"Composer\\DependencyResolver\\Operation\\UpdateOperation" => {
            return Ok(operation_construction_result(AnyOperation::Update(
                crate::dependency_resolver::operation::UpdateOperation::new(
                    package_arg(0)?,
                    package_arg(1)?,
                ),
            )));
        }
        b"Composer\\DependencyResolver\\Operation\\UninstallOperation" => {
            return Ok(operation_construction_result(AnyOperation::Uninstall(
                crate::dependency_resolver::operation::UninstallOperation::new(package_arg(0)?),
            )));
        }
        b"Composer\\DependencyResolver\\Operation\\MarkAliasInstalledOperation" => {
            return Ok(operation_construction_result(
                AnyOperation::MarkAliasInstalled(
                    crate::dependency_resolver::operation::MarkAliasInstalledOperation::new(
                        alias_package_arg(0)?,
                    ),
                ),
            ));
        }
        b"Composer\\DependencyResolver\\Operation\\MarkAliasUninstalledOperation" => {
            return Ok(operation_construction_result(
                AnyOperation::MarkAliasUninstalled(
                    crate::dependency_resolver::operation::MarkAliasUninstalledOperation::new(
                        alias_package_arg(0)?,
                    ),
                ),
            ));
        }
        // A Filesystem shares no state with the object graph beyond the process executor it
        // runs subprocesses through, so a plugin-built one is a complete instance rather than
        // a second view on a Rust-side service.
        b"Composer\\Util\\Filesystem" => {
            let executor = match ctor_args.first() {
                None | Some(PluginValue::Null) => None,
                _ => Some(process_executor_arg(0)?),
            };
            let rhandle = register_entity(RustEntity::Filesystem(std::rc::Rc::new(
                std::cell::RefCell::new(crate::util::Filesystem::new(executor)),
            )));
            return Ok(construction_result(rhandle));
        }
        // The job queue and the async permits of a process executor are its own state, and the
        // timeout it runs children under is the Rust side's; a plugin-built one is a complete
        // instance of the former holding the latter, not a second view on the graph's executor.
        b"Composer\\Util\\ProcessExecutor" => {
            let io = match ctor_args.first() {
                None | Some(PluginValue::Null) => None,
                _ => Some(io_arg(0)?),
            };
            let rhandle = register_entity(RustEntity::ProcessExecutor(std::rc::Rc::new(
                std::cell::RefCell::new(crate::util::ProcessExecutor::new(io)),
            )));
            return Ok(construction_result(rhandle));
        }
        // A downloader carries its own options, TLS defaults and request backends; what it
        // shares with the graph is the IO it collects authentication into and the config it
        // reads, both of which it receives as the proxies the plugin already holds.
        b"Composer\\Util\\HttpDownloader" => {
            let rhandle = register_entity(RustEntity::HttpDownloader(std::rc::Rc::new(
                std::cell::RefCell::new(crate::util::HttpDownloader::new(
                    io_arg(0)?,
                    config_arg(1)?,
                    arg_or::<IndexMap<BString, PhpMixed>>(
                        class_name,
                        ctor_args,
                        2,
                        IndexMap::new(),
                    )?,
                    arg_or::<bool>(class_name, ctor_args, 3, false)?,
                )),
            )));
            return Ok(construction_result(rhandle));
        }
        // TODO(plugin): the remaining proxied classes get a construction story on demand,
        // driven by explicit errors from real plugins. Each one has to decide what a
        // plugin-built instance means for the Rust-side graph, which is why none of them is
        // answered generically here.
        _ => {
            return Err(PhpThrow::runtime(bconcat!(
                "Shirabe does not support constructing ",
                class_name,
                " inside the plugin process yet"
            )));
        }
    };
    let rhandle = register_entity(RustEntity::Package(std::rc::Rc::new(
        std::cell::RefCell::new(package),
    )));
    Ok(construction_result(rhandle))
}

fn operation_construction_result(operation: AnyOperation) -> PluginValue {
    construction_result(register_entity(RustEntity::Operation(std::rc::Rc::new(
        operation,
    ))))
}

/// The rhandle a proxy stub's constructor binds itself to.
fn construction_result(rhandle: u64) -> PluginValue {
    PluginValue::Int(rhandle as i64)
}

/// Serves the `__clone` forwarder every proxy stub carries. Only entities whose Rust type
/// models the PHP clone answer; the rest are an explicit error, so a plugin cloning a live
/// service never silently ends up with two stubs over one entity.
fn clone_entity(entity: &RustEntity) -> Result<PluginValue, PhpThrow> {
    let rhandle = match entity {
        // `AnyPackage::dup` carries `BasePackage::__clone` (repository reset, id = -1) and the
        // `RootAliasPackage::__clone` override.
        RustEntity::Package(package) => {
            let cloned = package.borrow().dup();
            register_entity(RustEntity::Package(std::rc::Rc::new(
                std::cell::RefCell::new(cloned),
            )))
        }
        RustEntity::Composer(_)
        | RustEntity::AutoloadGenerator(_)
        | RustEntity::Config(_)
        | RustEntity::DownloadManager(_)
        | RustEntity::Filesystem(_)
        | RustEntity::Io(_)
        | RustEntity::InstallationManager(_)
        | RustEntity::Locker(_)
        | RustEntity::RepositoryManager(_)
        | RustEntity::Repository(_)
        | RustEntity::EventDispatcher(_)
        | RustEntity::Operation(_)
        | RustEntity::Transaction(_)
        | RustEntity::Plugin(_)
        | RustEntity::ProcessExecutor(_)
        | RustEntity::HttpDownloader(_)
        | RustEntity::Process(_) => {
            return Err(PhpThrow::runtime(
                "cloning this Rust-side entity over RPC is not supported",
            ));
        }
    };
    Ok(PluginValue::Int(rhandle as i64))
}

/// Serves the `__get`/`__set`/`__isset`/`__unset` forwarders every proxy stub carries.
fn dispatch_property_access(
    entity: &RustEntity,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let property = arg::<BString>(method_name, args, 0)?;
    // `BasePackage::$id` is the one instance property the entities expose so far.
    if let RustEntity::Package(package) = entity
        && property == "id"
    {
        return match method_name.as_bytes() {
            b"__get" => Ok(package.borrow().as_package_interface().get_id().to_wire()),
            b"__isset" => Ok(PluginValue::Bool(true)),
            b"__set" => {
                let id = arg::<i64>(method_name, args, 1)?;
                package.borrow_mut().as_package_interface_mut().set_id(id);
                Ok(PluginValue::Null)
            }
            b"__unset" => Err(PhpThrow::runtime(
                "the package property `id` cannot be unset over RPC",
            )),
            _ => unreachable!(),
        };
    }
    // TODO(plugin): the instance properties the proxied classes expose are widened on demand,
    // driven by these explicit errors from real plugins. Each one has to decide how the state
    // the real class keeps in that property is served from the Rust-side entity.
    Err(PhpThrow::runtime(bconcat!(
        "the property `",
        property,
        "` is not available over RPC yet"
    )))
}

/// Serves the static methods a proxy stub forwards instead of running locally, because their real
/// bodies reach state the Rust side owns or classes the worker has no code for.
pub(crate) fn call_static_entity(args: &[PluginValue]) -> Result<PluginValue, PhpThrow> {
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
            return Err(PhpThrow::runtime(bconcat!(
                "__shirabe_call_static expects a class name, a method name and an argument list, \
                 got ",
                display_to_bytes(format_args!("{other:?}"))
            )));
        }
    };
    let name = bconcat!(class, "::", method);
    let string_arg = |position: usize| arg::<BString>(name.as_bstr(), &call_args, position);
    match (class.as_slice(), method.as_slice()) {
        (b"Composer\\Util\\Filesystem", b"isLocalPath") => {
            Ok(crate::util::Filesystem::is_local_path(&string_arg(0)?).to_wire())
        }
        (b"Composer\\Util\\Filesystem", b"getPlatformPath") => Ok(PluginValue::string(
            path_to_bytes(&crate::util::Filesystem::get_platform_path(&string_arg(0)?)),
        )),
        (b"Composer\\Package\\Locker", b"getContentHash") => {
            crate::package::Locker::get_content_hash(string_arg(0)?)
                .map(PluginValue::string)
                .map_err(|error| error_throw(b"getContentHash failed", &error))
        }
        (b"Composer\\Util\\ProcessExecutor", b"getTimeout") => {
            Ok(crate::util::ProcessExecutor::get_timeout().to_wire())
        }
        (b"Composer\\Util\\ProcessExecutor", b"setTimeout") => {
            crate::util::ProcessExecutor::set_timeout(arg::<i64>(name.as_bstr(), &call_args, 0)?);
            Ok(PluginValue::Null)
        }
        // `escape(string|false|null $argument)` casts its argument to string first, which turns
        // both of the non-string forms into the empty string.
        (b"Composer\\Util\\ProcessExecutor", b"escape") => {
            let argument = match call_args.first() {
                None | Some(PluginValue::Null) | Some(PluginValue::Bool(false)) => {
                    BString::default()
                }
                _ => string_arg(0)?,
            };
            Ok(PluginValue::string(crate::util::ProcessExecutor::escape(
                &argument,
            )))
        }
        _ => Err(PhpThrow::runtime(bconcat!(
            "the static method ",
            name,
            " is not available over RPC yet"
        ))),
    }
}

/// The failure of a method of a proxied class the Rust side does not serve. `class` names the
/// class the way the message reads, such as "repository" or "IO".
pub(crate) fn unavailable_method(class: impl AsRef<[u8]>, method: impl AsRef<[u8]>) -> PhpThrow {
    PhpThrow::runtime(bconcat!(
        "the ",
        class.as_ref(),
        " method `",
        method.as_ref(),
        "` is not available over RPC yet"
    ))
}

/// The `Throw` a failed Rust-side call crosses the boundary as. An error carrying a ported PHP
/// exception keeps that exception's class, code and declared state, so a plugin catches what it
/// would catch under Composer; one carrying no exception keeps the `RuntimeException` shape,
/// with `context` naming the call that failed.
pub(crate) fn error_throw(context: impl AsRef<[u8]>, error: &anyhow::Error) -> PhpThrow {
    match AnyThrowable::of(error) {
        Some(exception) => exception_into_throw(exception, error),
        None => PhpThrow::runtime(bconcat!(context.as_ref(), ": ", error_chain_message(error))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_method_name_that_is_not_utf8_reaches_the_error_unchanged() {
        let name = b"method\xff";
        let throw = PluginRpcDispatcher::default()
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
