//! PHP-backed `PluginInterface` adapter and the R table serving its RPC callbacks.
//!
//! This module has no Composer counterpart: it is the Rust half of the plugin runtime split
//! (a real PHP child process executes the plugin code, see `docs/dev/php-worker.md`). The plugin
//! entity lives in the worker's P table; Rust-side entities the plugin can call back into
//! (`$composer`, `$io`) live in the R table here.

use crate::autoload::{AutoloadGeneratorInterface, ClassLoader};
use crate::command::BaseCommand;
use crate::composer::ComposerHandle;
use crate::dependency_resolver::AnyTransaction;
use crate::dependency_resolver::operation::AnyOperation;
use crate::event_dispatcher::event_dispatcher::dispatch_event_method;
use crate::event_dispatcher::{EventInterface, EventSubscriberInterface, SubscribedEventEntry};
use crate::filter::platform_requirement_filter::PlatformRequirementFilterFactory;
use crate::installer::InstallationManagerInterface;
use crate::io::IOInterface;
use crate::package::handle::AnyPackage;
use crate::package::locker::LockerInterface;
use crate::package::{DisplayMode, PackageInterfaceHandle, RootPackageInterfaceHandle};
use crate::plugin::capability::{Capability, CommandProvider};
use crate::plugin::capable::Capable;
use crate::plugin::job_registry;
use crate::plugin::job_registry::JobOwner;
use crate::plugin::php_plugin_value::{
    class_map_to_wire, date_time_from_wire, date_time_to_wire, link_from_wire, link_to_wire,
    response_to_wire,
};
use crate::plugin::plugin_interface::PluginInterface;
use crate::plugin::post_file_download_event::PostFileDownloadEvent;
use crate::plugin::pre_file_download_event::{FileDownloadContext, PreFileDownloadEvent};
use crate::plugin::pre_pool_create_event::PrePoolCreateEvent;
use crate::plugin::throwable_bridge::{exception_into_throw, throw_into_error, unwrap_php_result};
use crate::repository::{
    InstalledArrayRepository, InstalledFilesystemRepository, InstalledRepositoryInterfaceHandle,
    LockArrayRepository, RepositoryInterfaceHandle, RepositoryManagerInterface,
};
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_rpc::{
    PhpObjHandle, PhpThrow, PluginValue, RustMethodDispatcher, RustObjHandle,
    call_function_with_dispatcher, call_php_method, new_object, release_php_handle,
};
use shirabe_php_shim::{
    AnyThrowable, Catch as _, LogicException, PhpClass, PhpMixed, RuntimeException,
    UnexpectedValueException, bconcat, display_to_bytes, error_chain_message, path_from_bytes,
    path_to_bytes, php_str,
};
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::{InputInterface, InputValue};
use shirabe_symfony_console::output::OutputInterface;

/// A Rust-side entity a PHP proxy stub points back to.
#[derive(Debug, Clone)]
enum RustEntity {
    Composer(ComposerHandle),
    AutoloadGenerator(std::rc::Rc<std::cell::RefCell<dyn AutoloadGeneratorInterface>>),
    Config(std::rc::Rc<std::cell::RefCell<crate::config::Config>>),
    DownloadManager(
        std::rc::Rc<std::cell::RefCell<dyn crate::downloader::DownloadManagerInterface>>,
    ),
    Filesystem(std::rc::Rc<std::cell::RefCell<crate::util::Filesystem>>),
    Io(std::rc::Rc<std::cell::RefCell<dyn IOInterface>>),
    InstallationManager(std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>>),
    Locker(std::rc::Rc<std::cell::RefCell<dyn LockerInterface>>),
    RepositoryManager(std::rc::Rc<std::cell::RefCell<dyn RepositoryManagerInterface>>),
    Repository(RepositoryInterfaceHandle),
    Package(std::rc::Rc<std::cell::RefCell<AnyPackage>>),
    EventDispatcher(
        std::rc::Rc<std::cell::RefCell<dyn crate::event_dispatcher::EventDispatcherInterface>>,
    ),
    Operation(std::rc::Rc<AnyOperation>),
    Transaction(std::rc::Rc<AnyTransaction>),
    Plugin(std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>),
    ProcessExecutor(std::rc::Rc<std::cell::RefCell<crate::util::ProcessExecutor>>),
    HttpDownloader(std::rc::Rc<std::cell::RefCell<crate::util::HttpDownloader>>),
    /// A child a `ProcessExecutor` job ran to its exit, behind a `Shirabe\RustProcessStub`.
    Process(std::rc::Rc<std::cell::RefCell<shirabe_symfony_process::Process>>),
}

/// The pointer identity backing R-table interning: the same shared instance must always cross
/// the boundary as the same handle (`===` in the child).
fn entity_ptr_id(entity: &RustEntity) -> usize {
    match entity {
        RustEntity::Composer(composer) => {
            std::rc::Rc::as_ptr(composer.as_rc()) as *const () as usize
        }
        RustEntity::AutoloadGenerator(generator) => {
            std::rc::Rc::as_ptr(generator) as *const () as usize
        }
        RustEntity::Config(config) => std::rc::Rc::as_ptr(config) as *const () as usize,
        RustEntity::DownloadManager(dm) => std::rc::Rc::as_ptr(dm) as *const () as usize,
        RustEntity::Filesystem(fs) => std::rc::Rc::as_ptr(fs) as *const () as usize,
        RustEntity::Io(io) => std::rc::Rc::as_ptr(io) as *const () as usize,
        RustEntity::InstallationManager(im) => std::rc::Rc::as_ptr(im) as *const () as usize,
        RustEntity::Locker(locker) => std::rc::Rc::as_ptr(locker) as *const () as usize,
        RustEntity::RepositoryManager(rm) => std::rc::Rc::as_ptr(rm) as *const () as usize,
        RustEntity::Repository(repository) => repository.ptr_id(),
        RustEntity::Package(package) => std::rc::Rc::as_ptr(package) as *const () as usize,
        RustEntity::EventDispatcher(dispatcher) => {
            std::rc::Rc::as_ptr(dispatcher) as *const () as usize
        }
        RustEntity::Operation(operation) => std::rc::Rc::as_ptr(operation) as *const () as usize,
        RustEntity::Transaction(transaction) => {
            std::rc::Rc::as_ptr(transaction) as *const () as usize
        }
        RustEntity::Plugin(plugin) => std::rc::Rc::as_ptr(plugin) as *const () as usize,
        RustEntity::ProcessExecutor(process) => std::rc::Rc::as_ptr(process) as *const () as usize,
        RustEntity::HttpDownloader(downloader) => {
            std::rc::Rc::as_ptr(downloader) as *const () as usize
        }
        RustEntity::Process(process) => std::rc::Rc::as_ptr(process) as *const () as usize,
    }
}

thread_local! {
    /// The R table. Entries are strong references held while the child-side stub lives; a
    /// ReleaseRustHandle notification (sent by the stub's `__destruct`) removes the entry.
    /// Handles are monotonically increasing and never reused, so a released handle can never
    /// be confused with a later entity (no generation counter is needed).
    /// TODO(plugin): thread-local while the worker and its stub intern table are
    /// process-global; the session lock serializes calls, and every dispatch currently runs on
    /// the thread that registered the handle, but a handle minted on one thread is invisible
    /// to another.
    static R_TABLE: std::cell::RefCell<IndexMap<u64, RustEntity>> =
        std::cell::RefCell::new(IndexMap::new());

    static RELEASE_HOOK_INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Installs the ReleaseRustHandle listener that drops R-table entries when the child-side stub
/// is destructed. Idempotent; called by every entity registration.
fn ensure_release_hook_installed() {
    RELEASE_HOOK_INSTALLED.with(|installed| {
        if !installed.get() {
            installed.set(true);
            shirabe_php_rpc::set_release_rust_handle_hook(|rhandle| {
                R_TABLE.with(|table| {
                    table.borrow_mut().shift_remove(&rhandle);
                });
            });
        }
    });
}

/// Registers an entity in the R table, interned by shared-pointer identity within its variant.
fn register_entity(entity: RustEntity) -> u64 {
    ensure_release_hook_installed();
    R_TABLE.with(|table| {
        let mut table = table.borrow_mut();
        let ptr = entity_ptr_id(&entity);
        let discriminant = std::mem::discriminant(&entity);
        for (rhandle, existing) in table.iter() {
            if std::mem::discriminant(existing) == discriminant && entity_ptr_id(existing) == ptr {
                return *rhandle;
            }
        }
        let rhandle = shirabe_php_rpc::alloc_rhandle();
        table.insert(rhandle, entity);
        rhandle
    })
}

/// Registers the Composer instance in the R table.
pub(crate) fn register_composer_entity(composer: &ComposerHandle) -> u64 {
    register_entity(RustEntity::Composer(composer.clone()))
}

/// Registers an IO instance in the R table.
pub(crate) fn register_io_entity(io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>) -> u64 {
    register_entity(RustEntity::Io(io.clone()))
}

/// Wire value handing a Rust-side `Config` to the child, interned in the R table.
pub(crate) fn config_handle_value(
    config: &std::rc::Rc<std::cell::RefCell<crate::config::Config>>,
) -> PluginValue {
    let rhandle = register_entity(RustEntity::Config(config.clone()));
    rust_handle_value(rhandle, "Composer\\Config")
}

/// A `RustHandle` wire descriptor for a freshly registered (or re-interned) entity.
pub(crate) fn rust_handle_value(rhandle: u64, class: impl AsRef<[u8]>) -> PluginValue {
    PluginValue::RustHandle(RustObjHandle {
        rhandle,
        class: BString::from(class.as_ref()),
        epoch: 0,
        snapshot: None,
    })
}

/// The proxy stub class matching a package's concrete variant.
fn package_stub_class(package: &std::rc::Rc<std::cell::RefCell<AnyPackage>>) -> &'static BStr {
    match &*package.borrow() {
        AnyPackage::Package(_) => php_str(b"Composer\\Package\\Package"),
        AnyPackage::CompletePackage(_) => php_str(b"Composer\\Package\\CompletePackage"),
        AnyPackage::RootPackage(_) => php_str(b"Composer\\Package\\RootPackage"),
        AnyPackage::AliasPackage(_) => php_str(b"Composer\\Package\\AliasPackage"),
        AnyPackage::CompleteAliasPackage(_) => php_str(b"Composer\\Package\\CompleteAliasPackage"),
        AnyPackage::RootAliasPackage(_) => php_str(b"Composer\\Package\\RootAliasPackage"),
    }
}

/// The proxy stub class matching a repository's concrete type.
fn repository_stub_class(
    repository: &RepositoryInterfaceHandle,
) -> Result<&'static BStr, PhpThrow> {
    if repository.is::<InstalledFilesystemRepository>() {
        Ok(php_str(
            b"Composer\\Repository\\InstalledFilesystemRepository",
        ))
    } else if repository.is::<InstalledArrayRepository>() {
        Ok(php_str(b"Composer\\Repository\\InstalledArrayRepository"))
    } else if repository.is::<LockArrayRepository>() {
        Ok(php_str(b"Composer\\Repository\\LockArrayRepository"))
    } else {
        // TODO(plugin): the remaining repository classes get stubs on demand, driven by
        // explicit errors from real plugins.
        Err(runtime_throw(
            "no proxy stub class is available for this repository over RPC yet",
        ))
    }
}

/// The proxy stub class matching a solver operation's concrete type.
fn operation_stub_class(operation: &AnyOperation) -> &'static BStr {
    match operation {
        AnyOperation::Install(_) => {
            php_str(b"Composer\\DependencyResolver\\Operation\\InstallOperation")
        }
        AnyOperation::Update(_) => {
            php_str(b"Composer\\DependencyResolver\\Operation\\UpdateOperation")
        }
        AnyOperation::Uninstall(_) => {
            php_str(b"Composer\\DependencyResolver\\Operation\\UninstallOperation")
        }
        AnyOperation::MarkAliasInstalled(_) => {
            php_str(b"Composer\\DependencyResolver\\Operation\\MarkAliasInstalledOperation")
        }
        AnyOperation::MarkAliasUninstalled(_) => {
            php_str(b"Composer\\DependencyResolver\\Operation\\MarkAliasUninstalledOperation")
        }
    }
}

/// Registers a package and returns its wire descriptor.
fn package_handle_value(package: &std::rc::Rc<std::cell::RefCell<AnyPackage>>) -> PluginValue {
    let class = package_stub_class(package);
    let rhandle = register_entity(RustEntity::Package(package.clone()));
    rust_handle_value(rhandle, class)
}

/// Registers a Rust-implemented plugin and returns its wire descriptor. A PHP-implemented
/// plugin never takes this route: its entity lives in the child's P table already.
pub fn plugin_handle_value(
    plugin: &std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
) -> PluginValue {
    // TODO(plugin): a Rust-implemented plugin that is also an EventSubscriberInterface crosses
    // as one of these two classes, so `instanceof EventSubscriberInterface` is false in the
    // child; Composer's own subscriber dispatch runs on the Rust side and never asks.
    let class = match plugin.borrow().as_capable() {
        Some(_) => php_str(b"Shirabe\\RustCapablePluginStub"),
        None => php_str(b"Shirabe\\RustPluginStub"),
    };
    let rhandle = register_entity(RustEntity::Plugin(plugin.clone()));
    rust_handle_value(rhandle, class)
}

/// Registers a transaction and returns its wire descriptor.
pub(crate) fn transaction_handle_value(transaction: &std::rc::Rc<AnyTransaction>) -> PluginValue {
    let class = match &**transaction {
        AnyTransaction::Transaction(_) => php_str(b"Composer\\DependencyResolver\\Transaction"),
        AnyTransaction::LocalRepoTransaction(_) => {
            php_str(b"Composer\\DependencyResolver\\LocalRepoTransaction")
        }
    };
    let rhandle = register_entity(RustEntity::Transaction(transaction.clone()));
    rust_handle_value(rhandle, class)
}

/// Registers a solver operation and returns its wire descriptor.
pub(crate) fn operation_handle_value(operation: &std::rc::Rc<AnyOperation>) -> PluginValue {
    let class = operation_stub_class(operation);
    let rhandle = register_entity(RustEntity::Operation(operation.clone()));
    rust_handle_value(rhandle, class)
}

/// The PHP class name (= proxy stub class) of a Rust IO instance, for the `__class` field of
/// its handle descriptor.
pub(crate) fn io_stub_class(
    io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
) -> anyhow::Result<&'static BStr> {
    let borrowed = io.borrow();
    let any = borrowed.as_any();
    if any
        .downcast_ref::<crate::io::buffer_io::BufferIO>()
        .is_some()
    {
        Ok(BStr::new("Composer\\IO\\BufferIO"))
    } else if any
        .downcast_ref::<crate::io::console_io::ConsoleIO>()
        .is_some()
    {
        Ok(BStr::new("Composer\\IO\\ConsoleIO"))
    } else if any.downcast_ref::<crate::io::null_io::NullIO>().is_some() {
        Ok(BStr::new("Composer\\IO\\NullIO"))
    } else {
        // TODO(plugin): only IO implementations with a generated proxy stub can cross the
        // boundary; the rest are an explicit error until stubs of their own are generated.
        Err(anyhow::anyhow!(
            "no proxy stub class is available for this IO implementation"
        ))
    }
}

/// Looks a class up in every registered Rust-side `ClassLoader`, in registration order — the
/// Rust mirror of what the PHP `spl_autoload_register` stack would do in-process.
///
/// TODO(plugin): `ClassLoader::register` keeps one loader per vendor-dir (matching upstream's
/// `$registeredLoaders`), but the real spl stack keeps every registered loader; because the
/// `spl_autoload_register` shim is a no-op, registering a second plugin loader under the same
/// vendor-dir evicts the first one here, and a class of the earlier plugin that was never
/// loaded can become unresolvable (PHP would still find it).
pub(crate) fn find_file_in_registered_loaders(class: impl AsRef<[u8]>) -> Option<BString> {
    let class = class.as_ref();
    for (_vendor_dir, mut loader) in ClassLoader::get_registered_loaders() {
        if let Some(file) = loader.find_file(class) {
            return Some(file);
        }
    }
    None
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
        _out_param_positions: &[u32],
        out_params: &mut IndexMap<u32, PluginValue>,
    ) -> Result<PluginValue, PhpThrow> {
        if rhandle == 0 {
            if method_name == "__shirabe_find_file" {
                let class = arg::<BString>(method_name, &args, 0)?;
                return Ok(find_file_in_registered_loaders(&class).to_plugin_value());
            }
            if method_name == "__shirabe_run_rust_command" {
                let name = arg::<BString>(method_name, &args, 0)?;
                let input_line = arg::<BString>(method_name, &args, 1)?;
                return match crate::console::application::run_worker_reverse_command(
                    name.as_ref(),
                    input_line.as_ref(),
                ) {
                    Ok(code) => Ok(code.to_plugin_value()),
                    Err(e) => Err(runtime_throw(bconcat!(error_chain_message(&e)))),
                };
            }
            if method_name == "__shirabeConstruct" {
                return construct_entity(&args);
            }
            if method_name == "__shirabeCallStatic" {
                return call_static_entity(&args);
            }
            if let Some(answer) = dispatch_job_service_method(method_name, &args) {
                return answer;
            }
            return Err(runtime_throw(bconcat!(
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
        b"__shirabeResolveJob" => job_id().map(|id| {
            let value = args.get(1).cloned().unwrap_or(PluginValue::Null);
            job_registry::settle_php_job(id, Ok(value));
            PluginValue::Null
        }),
        b"__shirabeRejectJob" => (|| {
            let id = job_id()?;
            let mut throw = PhpThrow::new(
                arg::<BString>(method_name, args, 1)?,
                arg::<BString>(method_name, args, 2)?,
                arg::<i64>(method_name, args, 3)?,
            );
            *throw.properties = match args.get(4) {
                Some(PluginValue::Array(properties)) => properties.clone(),
                _ => IndexMap::new(),
            };
            throw.file = arg::<bstr::BString>(method_name, args, 5)?;
            throw.line = arg::<i64>(method_name, args, 6)?;
            throw.parents = arg::<Vec<bstr::BString>>(method_name, args, 7)?;
            job_registry::settle_php_job(id, Err(throw));
            Ok(PluginValue::Null)
        })(),
        b"__shirabeCancelJob" => job_id().map(|id| match job_registry::cancel(id) {
            Some(throw) => PluginValue::List(job_registry::throw_fields(throw)),
            None => PluginValue::Null,
        }),
        b"__shirabeAbortJobs" => {
            job_registry::abort_internal_jobs(None);
            Ok(PluginValue::Null)
        }
        _ => return None,
    })
}

/// Polls a job future that was just built, the one time a job is advanced outside a tick.
/// Composer starts a process inside `executeAsync()` itself when the job budget has room, and
/// this poll is what takes the future that far.
fn poll_once<T>(
    future: &mut std::pin::Pin<Box<dyn std::future::Future<Output = T>>>,
) -> std::task::Poll<T> {
    crate::util::sync_executor::block_on(std::future::poll_fn(|cx| {
        std::task::Poll::Ready(future.as_mut().poll(cx))
    }))
}

/// How long one tick of each service may block with no job settling, matching the granularity of
/// Composer's own wait loops: the `curl_multi_select()` timeout of `CurlDownloader::tick()`, and
/// the `usleep()` between the ticks of `ProcessExecutor::wait()`.
const HTTP_DOWNLOADER_TICK_CAP: std::time::Duration = std::time::Duration::from_secs(5);
const PROCESS_EXECUTOR_TICK_CAP: std::time::Duration = std::time::Duration::from_millis(1);

/// Serves `wait(?int $index)` and `countActiveJobs(?int $index)` of the two services, whose
/// `$index` is a job id.
fn dispatch_tick_method(
    owner: JobOwner,
    cap: std::time::Duration,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let index = match args.first() {
        None | Some(PluginValue::Null) => None,
        _ => Some(arg::<i64>(method_name, args, 0)? as u64),
    };
    if method_name == "countActiveJobs" {
        return Ok(PluginValue::Int(job_registry::tick(owner, cap, index)?));
    }
    while job_registry::tick(owner, cap, index)? != 0 {}
    Ok(PluginValue::Null)
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
    if method_name == "__shirabeClone" {
        return match entity {
            Some(entity) => clone_entity(&entity),
            None => Err(runtime_throw(bconcat!("unknown Rust handle ", rhandle))),
        };
    }
    if matches!(
        method_name.as_bytes(),
        b"__get" | b"__set" | b"__isset" | b"__unset"
    ) {
        return match entity {
            Some(entity) => dispatch_property_access(&entity, method_name, args),
            None => Err(runtime_throw(bconcat!("unknown Rust handle ", rhandle))),
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
        None => Err(runtime_throw(bconcat!("unknown Rust handle ", rhandle))),
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
            return Err(runtime_throw(bconcat!(
                "__shirabeConstruct expects a class name and an argument list, got ",
                display_to_bytes(format_args!("{other:?}"))
            )));
        }
    };
    let class_name = class.as_bstr();
    let string_arg = |position: usize| arg::<BString>(class_name, ctor_args, position);
    let package_arg =
        |position: usize| arg::<PackageInterfaceHandle>(class_name, ctor_args, position);
    let io_arg = |position: usize| {
        arg::<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>(class_name, ctor_args, position)
    };
    let process_executor_arg = |position: usize| {
        arg::<std::rc::Rc<std::cell::RefCell<crate::util::ProcessExecutor>>>(
            class_name, ctor_args, position,
        )
    };
    let config_arg = |position: usize| {
        arg::<std::rc::Rc<std::cell::RefCell<crate::config::Config>>>(
            class_name, ctor_args, position,
        )
    };
    let alias_package_arg =
        |position: usize| -> Result<crate::package::AliasPackageHandle, PhpThrow> {
            package_arg(position)?
                .as_alias()
                .ok_or_else(|| runtime_throw(bconcat!(class_name, " expects an AliasPackage")))
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
                runtime_throw(bconcat!(class_name, " expects a real Package to alias"))
            })?;
            AnyPackage::AliasPackage(crate::package::AliasPackage::new(
                alias_of,
                string_arg(1)?,
                string_arg(2)?,
            ))
        }
        b"Composer\\Package\\CompleteAliasPackage" => {
            let alias_of = package_arg(0)?.as_complete_package().ok_or_else(|| {
                runtime_throw(bconcat!(
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
                runtime_throw(bconcat!(class_name, " expects a real RootPackage to alias"))
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
            return Err(runtime_throw(bconcat!(
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

/// The `[$rhandle, $epoch]` pair a proxy stub's constructor binds itself to.
fn construction_result(rhandle: u64) -> PluginValue {
    PluginValue::List(vec![PluginValue::Int(rhandle as i64), PluginValue::Int(0)])
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
            return Err(runtime_throw(
                "cloning this Rust-side entity over RPC is not supported",
            ));
        }
    };
    Ok(PluginValue::List(vec![
        PluginValue::Int(rhandle as i64),
        PluginValue::Int(0),
    ]))
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
            b"__get" => Ok(package
                .borrow()
                .as_package_interface()
                .get_id()
                .to_plugin_value()),
            b"__isset" => Ok(PluginValue::Bool(true)),
            b"__set" => {
                let id = arg::<i64>(method_name, args, 1)?;
                package.borrow_mut().as_package_interface_mut().set_id(id);
                Ok(PluginValue::Null)
            }
            _ => Err(runtime_throw(
                "the package property `id` cannot be unset over RPC",
            )),
        };
    }
    // TODO(plugin): the instance properties the proxied classes expose are widened on demand,
    // driven by these explicit errors from real plugins. Each one has to decide how the state
    // the real class keeps in that property is served from the Rust-side entity.
    Err(runtime_throw(bconcat!(
        "the property `",
        property,
        "` is not available over RPC yet"
    )))
}

fn dispatch_plugin_method(
    plugin: &std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
    method_name: &BStr,
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getCapabilities" => {
            let plugin = plugin.borrow();
            let capable = plugin.as_capable().ok_or_else(|| {
                runtime_throw(bconcat!(
                    "plugin ",
                    plugin.get_class_name(),
                    " does not implement Capable"
                ))
            })?;
            let capabilities = capable
                .get_capabilities()
                .map_err(|error| error_throw(b"getCapabilities failed", &error))?;
            Ok(capabilities.to_plugin_value())
        }
        // TODO(plugin): the lifecycle methods would have to turn the `$composer`/`$io` stubs the
        // child passes back into the Rust handles they stand for; nothing calls them, because
        // Composer activates a Rust-implemented plugin on the Rust side.
        other => Err(runtime_throw(bconcat!(
            "the plugin method `",
            other,
            "` is not available over RPC yet"
        ))),
    }
}

fn dispatch_composer_method(
    composer: &ComposerHandle,
    method_name: &BStr,
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getRepositoryManager" => {
            let rm = composer.borrow().get_repository_manager();
            let rhandle = register_entity(RustEntity::RepositoryManager(rm));
            Ok(rust_handle_value(
                rhandle,
                "Composer\\Repository\\RepositoryManager",
            ))
        }
        b"getInstallationManager" => {
            let im = composer.borrow().get_installation_manager();
            let rhandle = register_entity(RustEntity::InstallationManager(im));
            Ok(rust_handle_value(
                rhandle,
                "Composer\\Installer\\InstallationManager",
            ))
        }
        b"getPackage" => {
            let package = composer.borrow().get_package().as_rc().clone();
            Ok(package_handle_value(&package))
        }
        b"getEventDispatcher" => {
            let dispatcher = composer.borrow().get_event_dispatcher();
            let rhandle = register_entity(RustEntity::EventDispatcher(dispatcher));
            Ok(rust_handle_value(
                rhandle,
                "Composer\\EventDispatcher\\EventDispatcher",
            ))
        }
        b"getConfig" => Ok(config_handle_value(&composer.borrow().get_config())),
        b"getAutoloadGenerator" => {
            let generator = composer.borrow().get_autoload_generator();
            let rhandle = register_entity(RustEntity::AutoloadGenerator(generator));
            Ok(rust_handle_value(
                rhandle,
                "Composer\\Autoload\\AutoloadGenerator",
            ))
        }
        b"getDownloadManager" => {
            let dm = composer.borrow().get_download_manager();
            let rhandle = register_entity(RustEntity::DownloadManager(dm));
            Ok(rust_handle_value(
                rhandle,
                "Composer\\Downloader\\DownloadManager",
            ))
        }
        // `getLoop()` itself never reaches here: the Loop is a two-world class, and the stub builds
        // the worker-side one over the two services the Rust-side Loop drives.
        b"__shirabeLoopServices" => {
            let r#loop = composer.borrow().get_loop();
            let downloader = r#loop.borrow().get_http_downloader().clone();
            let process = r#loop.borrow().get_process_executor().cloned();
            Ok(PluginValue::List(vec![
                rust_handle_value(
                    register_entity(RustEntity::HttpDownloader(downloader)),
                    "Composer\\Util\\HttpDownloader",
                ),
                match process {
                    Some(process) => rust_handle_value(
                        register_entity(RustEntity::ProcessExecutor(process)),
                        "Composer\\Util\\ProcessExecutor",
                    ),
                    None => PluginValue::Null,
                },
            ]))
        }
        b"getLocker" => {
            let locker = composer.borrow().get_locker();
            let rhandle = register_entity(RustEntity::Locker(locker));
            Ok(rust_handle_value(rhandle, "Composer\\Package\\Locker"))
        }
        // TODO(plugin): the remaining Composer object graph (getPluginManager, ...)
        // becomes reachable over RPC on demand, driven by explicit errors from real plugins.
        other => Err(runtime_throw(bconcat!(
            "the Composer method `",
            other,
            "` is not available over RPC yet"
        ))),
    }
}

fn dispatch_autoload_generator_method(
    generator: &std::rc::Rc<std::cell::RefCell<dyn AutoloadGeneratorInterface>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let flag = |position: usize| arg_or::<bool>(method_name, args, position, true);
    match method_name.as_bytes() {
        b"setDevMode" => generator.borrow().set_dev_mode(flag(0)?),
        b"setClassMapAuthoritative" => {
            generator
                .borrow()
                .set_class_map_authoritative(arg::<bool>(method_name, args, 0)?)
        }
        b"setApcu" => generator.borrow().set_apcu(
            arg::<bool>(method_name, args, 0)?,
            arg::<Option<BString>>(method_name, args, 1)?,
        ),
        b"setRunScripts" => generator.borrow().set_run_scripts(flag(0)?),
        b"setDryRun" => generator.borrow().set_dry_run(flag(0)?),
        b"setIgnorePlatformRequirements" => {
            // TODO(php-runtime): upstream raises an E_USER_DEPRECATED here
            // ("AutoloadGenerator::setIgnorePlatformRequirements is deprecated since Composer 2.2,
            // use setPlatformRequirementFilter instead."), which Composer's ErrorHandler prints as
            // a deprecation notice; the worker does not run that handler.
            let filter = PlatformRequirementFilterFactory::from_bool_or_list(arg::<PhpMixed>(
                method_name,
                args,
                0,
            )?)
            .map_err(|error| error_throw(b"setIgnorePlatformRequirements failed", &error))?;
            generator.borrow().set_platform_requirement_filter(filter);
        }
        b"dump" => {
            let config = arg::<std::rc::Rc<std::cell::RefCell<crate::config::Config>>>(
                method_name,
                args,
                0,
            )?;
            let local_repo = arg::<RepositoryInterfaceHandle>(method_name, args, 1)?;
            let root_package = root_package_arg(method_name, args, 2)?;
            let installation_manager = arg::<
                std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>>,
            >(method_name, args, 3)?;
            let target_dir = arg::<BString>(method_name, args, 4)?;
            let scan_psr_packages = arg_or::<bool>(method_name, args, 5, false)?;
            let suffix = arg::<Option<BString>>(method_name, args, 6)?;
            let locker = arg::<Option<std::rc::Rc<std::cell::RefCell<dyn LockerInterface>>>>(
                method_name,
                args,
                7,
            )?;
            let strict_ambiguous = arg_or::<bool>(method_name, args, 8, false)?;
            let class_map = generator
                .borrow()
                .dump(
                    &config.borrow(),
                    local_repo,
                    root_package,
                    installation_manager,
                    target_dir.as_bstr(),
                    scan_psr_packages,
                    suffix,
                    locker,
                    strict_ambiguous,
                )
                .map_err(|error| error_throw(b"dump failed over RPC", &error))?;
            return Ok(class_map_to_wire(&class_map));
        }
        b"buildPackageMap" => {
            let installation_manager = arg::<
                std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>>,
            >(method_name, args, 0)?;
            // TODO(type-model): PHP takes any PackageInterface as the root package here, while
            // the Rust signature narrows it to a RootPackageInterface.
            let root_package = root_package_arg(method_name, args, 1)?;
            let packages = arg::<Vec<PackageInterfaceHandle>>(method_name, args, 2)?;
            let package_map = generator
                .borrow()
                .build_package_map(installation_manager, root_package, packages)
                .map_err(|error| error_throw(b"buildPackageMap failed over RPC", &error))?;
            return Ok(package_map_value(package_map));
        }
        b"parseAutoloads" => {
            let package_map = package_map_arg(method_name, args, 0)?;
            let root_package = root_package_arg(method_name, args, 1)?;
            let filtered_dev_packages =
                arg_or::<PhpMixed>(method_name, args, 2, PhpMixed::Bool(false))?;
            return Ok(generator
                .borrow()
                .parse_autoloads(package_map, root_package, filtered_dev_packages)
                .to_plugin_value());
        }
        b"createLoader" => {
            let autoloads = arg::<IndexMap<BString, PhpMixed>>(method_name, args, 0)?;
            let vendor_dir = arg::<Option<BString>>(method_name, args, 1)?;
            let loader = generator
                .borrow()
                .create_loader(&autoloads, vendor_dir.clone())
                .map_err(|error| error_throw(b"createLoader failed over RPC", &error))?;
            return class_loader_to_php(&loader, vendor_dir)
                .map_err(|error| error_throw(b"createLoader failed over RPC", &error));
        }
        // TODO(plugin): no PlatformRequirementFilterInterface implementation crosses the boundary:
        // the factory's filters are guarded in the worker, and a plugin-implemented filter has no
        // Rust-side adapter.
        b"setPlatformRequirementFilter" => {
            return Err(runtime_throw(
                "AutoloadGenerator::setPlatformRequirementFilter() cannot take a filter over RPC yet",
            ));
        }
        other => {
            return Err(runtime_throw(bconcat!(
                "the AutoloadGenerator method `",
                other,
                "` is not available over RPC yet"
            )));
        }
    }
    Ok(PluginValue::Null)
}

/// Resolves a package argument the PHP signature types as a root package.
fn root_package_arg(
    method: &BStr,
    args: &[PluginValue],
    position: usize,
) -> Result<RootPackageInterfaceHandle, PhpThrow> {
    arg::<PackageInterfaceHandle>(method, args, position)?
        .as_root()
        .ok_or_else(|| {
            arg_throw(
                method,
                position,
                "a RootPackageInterface",
                args.get(position),
            )
        })
}

/// Decodes the `array{0: PackageInterface, 1: string|null}[]` package map `buildPackageMap()`
/// returns.
fn package_map_arg(
    method: &BStr,
    args: &[PluginValue],
    position: usize,
) -> Result<Vec<(PackageInterfaceHandle, Option<std::path::PathBuf>)>, PhpThrow> {
    let entries: Vec<&PluginValue> = match args.get(position) {
        Some(PluginValue::List(items)) => items.iter().collect(),
        Some(PluginValue::Array(map)) => map.values().collect(),
        other => return Err(arg_throw(method, position, "an array", other)),
    };
    entries
        .into_iter()
        .map(|entry| match entry {
            PluginValue::List(pair) if pair.len() == 2 => Ok((
                PackageInterfaceHandle::from_arg(method, position, pair.first())?,
                Option::<BString>::from_arg(method, position, pair.get(1))?
                    .map(|path| std::path::PathBuf::from(path_from_bytes(&path))),
            )),
            other => Err(arg_throw(
                method,
                position,
                "a list of [package, install path] pairs",
                Some(other),
            )),
        })
        .collect()
}

fn package_map_value(
    package_map: Vec<(PackageInterfaceHandle, Option<std::path::PathBuf>)>,
) -> PluginValue {
    PluginValue::List(
        package_map
            .into_iter()
            .map(|(package, install_path)| {
                PluginValue::List(vec![
                    package.to_plugin_value(),
                    install_path.to_plugin_value(),
                ])
            })
            .collect(),
    )
}

/// `ClassLoader` is the class the worker bootstraps its own autoloading with, so the child holds
/// the real implementation and the loader is built there: a fresh instance receives the state the
/// Rust-side loader ended up with, through the same methods `createLoader()` fills it with.
fn class_loader_to_php(
    loader: &ClassLoader,
    vendor_dir: Option<BString>,
) -> anyhow::Result<PluginValue> {
    let handle = new_php_object(
        BStr::new("Composer\\Autoload\\ClassLoader"),
        vec![vendor_dir.to_plugin_value()],
    )?;
    let add = |method: &[u8], prefix: &[u8], paths: &[BString]| {
        call_php_entity_method(
            &handle,
            BStr::new(method),
            vec![
                PluginValue::string(prefix),
                paths.to_vec().to_plugin_value(),
            ],
        )
        .map(|_| ())
    };
    for (prefix, paths) in loader.get_prefixes() {
        add(b"add", prefix.as_ref(), &paths)?;
    }
    if !loader.get_fallback_dirs().is_empty() {
        add(b"add", b"".as_slice(), loader.get_fallback_dirs())?;
    }
    for (prefix, paths) in loader.get_prefixes_psr4() {
        add(b"addPsr4", prefix.as_ref(), paths)?;
    }
    if !loader.get_fallback_dirs_psr4().is_empty() {
        add(b"addPsr4", b"".as_slice(), loader.get_fallback_dirs_psr4())?;
    }
    if !loader.get_class_map().is_empty() {
        call_php_entity_method(
            &handle,
            BStr::new("addClassMap"),
            vec![loader.get_class_map().clone().to_plugin_value()],
        )?;
    }
    Ok(PluginValue::PhpHandle(handle))
}

fn dispatch_config_method(
    config: &std::rc::Rc<std::cell::RefCell<crate::config::Config>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let key = |position: usize| arg::<BString>(method_name, args, position);
    let flags = |position: usize| arg_or::<i64>(method_name, args, position, 0);
    match method_name.as_bytes() {
        b"get" => {
            let value = config
                .borrow()
                .get_with_flags(&key(0)?, flags(1)?)
                .map_err(|error| error_throw(b"get failed over RPC", &error))?;
            Ok(value.to_plugin_value())
        }
        b"all" => {
            let all = config
                .borrow_mut()
                .all(flags(0)?)
                .map_err(|error| error_throw(b"all failed over RPC", &error))?;
            Ok(all.to_plugin_value())
        }
        b"raw" => Ok(config.borrow().raw().to_plugin_value()),
        b"has" => Ok(config.borrow().has(&key(0)?).to_plugin_value()),
        b"getRepositories" => Ok(config.borrow().get_repositories().to_plugin_value()),
        b"getSourceOfValue" => Ok(config
            .borrow_mut()
            .get_source_of_value(&key(0)?)
            .to_plugin_value()),
        b"merge" => {
            let values = arg::<IndexMap<BString, PhpMixed>>(method_name, args, 0)?;
            let source = arg_or::<BString>(
                method_name,
                args,
                1,
                crate::config::Config::SOURCE_UNKNOWN.to_owned(),
            )?;
            config.borrow_mut().merge(&values, &source);
            Ok(PluginValue::Null)
        }
        // TODO(plugin): the config-source surface (getConfigSource / setAuthConfigSource / ...)
        // needs proxy stubs for ConfigSourceInterface implementations, and prohibitUrlByConfig
        // needs the IO argument decoded back into the Rust-side instance; both are widened on
        // demand, driven by explicit errors from real plugins.
        other => Err(runtime_throw(bconcat!(
            "the Config method `",
            other,
            "` is not available over RPC yet"
        ))),
    }
}

/// The download manager's contract is asynchronous on both sides: PHP declares a
/// `PromiseInterface` return, Rust an `async fn`. Each call registers its future as a job and
/// answers with the job's pending promise.
fn dispatch_download_manager_method(
    dm: &std::rc::Rc<std::cell::RefCell<dyn crate::downloader::DownloadManagerInterface>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let string_arg = |position: usize| arg::<BString>(method_name, args, position);
    let path_arg = |position: usize| {
        arg::<bstr::BString>(method_name, args, position)
            .map(|path| std::path::PathBuf::from(path_from_bytes(&path)))
    };
    let package_arg = |position: usize| arg::<PackageInterfaceHandle>(method_name, args, position);
    let optional_package_arg =
        |position: usize| arg::<Option<PackageInterfaceHandle>>(method_name, args, position);
    let future: std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<Option<PhpMixed>>>>,
    > = match method_name.as_bytes() {
        b"setPreferSource" | b"setPreferDist" => {
            let preferred = arg::<bool>(method_name, args, 0)?;
            if method_name == "setPreferSource" {
                dm.borrow().set_prefer_source(preferred);
            } else {
                dm.borrow().set_prefer_dist(preferred);
            }
            return Ok(PluginValue::Null);
        }
        b"download" => {
            let (dm, package, path, prev) = (
                dm.clone(),
                package_arg(0)?,
                path_arg(1)?,
                optional_package_arg(2)?,
            );
            Box::pin(async move { dm.borrow().download(package, &path, prev).await })
        }
        b"prepare" => {
            let (dm, r#type, package, path, prev) = (
                dm.clone(),
                string_arg(0)?,
                package_arg(1)?,
                path_arg(2)?,
                optional_package_arg(3)?,
            );
            Box::pin(async move {
                dm.borrow()
                    .prepare(BStr::new(&r#type), package, &path, prev)
                    .await
            })
        }
        b"install" => {
            let (dm, package, path) = (dm.clone(), package_arg(0)?, path_arg(1)?);
            Box::pin(async move { dm.borrow().install(package, &path).await })
        }
        b"update" => {
            let (dm, initial, target, path) =
                (dm.clone(), package_arg(0)?, package_arg(1)?, path_arg(2)?);
            Box::pin(async move { dm.borrow().update(initial, target, &path).await })
        }
        b"remove" => {
            let (dm, package, path) = (dm.clone(), package_arg(0)?, path_arg(1)?);
            Box::pin(async move { dm.borrow().remove(package, &path).await })
        }
        b"cleanup" => {
            let (dm, r#type, package, path, prev) = (
                dm.clone(),
                string_arg(0)?,
                package_arg(1)?,
                path_arg(2)?,
                optional_package_arg(3)?,
            );
            Box::pin(async move {
                dm.borrow()
                    .cleanup(BStr::new(&r#type), package, &path, prev)
                    .await
            })
        }
        // TODO(plugin): the downloader-facing surface (getDownloader / setDownloader /
        // getDownloaderForPackage / getDownloaderType) needs proxy stubs for
        // DownloaderInterface implementations before it can cross.
        other => {
            return Err(runtime_throw(bconcat!(
                "the DownloadManager method `",
                other,
                "` is not available over RPC yet"
            )));
        }
    };

    // Under Composer the promise is a chain over the job the downloader started on the graph's
    // HttpDownloader, so that is the service whose `countActiveJobs()` keeps a wait going until
    // this job is done.
    let downloader = graph_http_downloader(dm)?;
    let started = std::rc::Rc::new(std::cell::Cell::new(false));
    let method = method_name.to_owned();
    Ok(job_registry::register(
        JobOwner::http_downloader(&downloader),
        downloader,
        started.clone(),
        // TODO(async): Composer's cancel reaches the request or the process at the root of the
        // chain and rejects with that job's own exception; the root is not known here.
        error_throw(
            BStr::new(""),
            &crate::exception::IrrecoverableDownloadException::new(bconcat!(
                "DownloadManager::",
                method_name,
                "() canceled"
            ))
            .into(),
        ),
        Box::pin(async move {
            started.set(true);
            future
                .await
                .map(|resolved| resolved.to_plugin_value())
                .map_err(|error| error_throw(bconcat!(method, " failed over RPC"), &error))
        }),
    ))
}

/// The HttpDownloader of the object graph `dm` belongs to: the one the Loop of its Composer
/// instance drives.
fn graph_http_downloader(
    dm: &std::rc::Rc<std::cell::RefCell<dyn crate::downloader::DownloadManagerInterface>>,
) -> Result<std::rc::Rc<std::cell::RefCell<crate::util::HttpDownloader>>, PhpThrow> {
    let composers: Vec<ComposerHandle> = R_TABLE.with(|table| {
        table
            .borrow()
            .values()
            .filter_map(|entity| match entity {
                RustEntity::Composer(composer) => Some(composer.clone()),
                _ => None,
            })
            .collect()
    });
    for composer in composers {
        let composer = composer.borrow();
        if std::rc::Rc::ptr_eq(&composer.get_download_manager(), dm) {
            return Ok(composer.get_loop().borrow().get_http_downloader().clone());
        }
    }
    Err(runtime_throw(
        "this DownloadManager belongs to no Composer instance the plugin process holds",
    ))
}

/// A `\React\Promise\PromiseInterface` already fulfilled with `value`, built in the worker so
/// PHP callers get the object type their signatures declare. The answer is the descriptor the
/// worker exchanges for that promise, to be passed on as the answer of the call being served.
fn resolved_promise(value: PluginValue) -> Result<PluginValue, PhpThrow> {
    match call_function_with_dispatcher(
        "__shirabe_resolved_promise",
        vec![value],
        Some(&mut PluginRpcDispatcher::default()),
    ) {
        Ok(Ok(promise)) => Ok(promise),
        Ok(Err(throw)) => Err(throw),
        Err(error) => Err(runtime_throw(bconcat!(
            "creating a resolved promise in the plugin process failed: ",
            error_chain_message(&error)
        ))),
    }
}

/// A `\React\Promise\PromiseInterface` already rejected with the exception `throw` describes,
/// built in the worker. A failed request reaches the plugin as a rejection it handles, the way it
/// does under Composer, rather than as a throw out of the call that started it.
fn rejected_promise(throw: PhpThrow) -> Result<PluginValue, PhpThrow> {
    match call_function_with_dispatcher(
        "__shirabe_rejected_promise",
        job_registry::throw_fields(throw),
        Some(&mut PluginRpcDispatcher::default()),
    ) {
        Ok(Ok(promise)) => Ok(promise),
        Ok(Err(throw)) => Err(throw),
        Err(error) => Err(runtime_throw(bconcat!(
            "creating a rejected promise in the plugin process failed: ",
            error_chain_message(&error)
        ))),
    }
}

/// Serves `Shirabe\RustProcessStub`: everything the stub answers about the terminated process
/// behind it, read in one round trip.
fn dispatch_process_method(
    process: &std::rc::Rc<std::cell::RefCell<shirabe_symfony_process::Process>>,
    method_name: &BStr,
) -> Result<PluginValue, PhpThrow> {
    if method_name != "__shirabeFacts" {
        return Err(runtime_throw(bconcat!(
            "unknown Process method `",
            method_name,
            "`"
        )));
    }
    let failed = |error: anyhow::Error| error_throw(b"reading the process failed", &error);
    let mut process = process.borrow_mut();
    let facts = [
        ("exitCode", process.get_exit_code().to_plugin_value()),
        (
            "exitCodeText",
            process.get_exit_code_text().to_plugin_value(),
        ),
        (
            "output",
            PluginValue::string(process.get_output().map_err(failed)?),
        ),
        (
            "errorOutput",
            PluginValue::string(process.get_error_output().map_err(failed)?),
        ),
        (
            "signaled",
            PluginValue::Bool(process.has_been_signaled().map_err(failed)?),
        ),
        (
            "termSignal",
            PluginValue::Int(process.get_term_signal().map_err(failed)?),
        ),
        (
            "stopped",
            PluginValue::Bool(process.has_been_stopped().map_err(failed)?),
        ),
        (
            "stopSignal",
            PluginValue::Int(process.get_stop_signal().map_err(failed)?),
        ),
        (
            "commandLine",
            PluginValue::string(process.get_command_line()),
        ),
        (
            "workingDirectory",
            process.get_working_directory().to_plugin_value(),
        ),
        (
            "env",
            PluginValue::from_php_mixed(&PhpMixed::Array(process.get_env().clone())),
        ),
        (
            "timeout",
            match process.get_timeout() {
                Some(timeout) => PluginValue::Float(timeout),
                None => PluginValue::Null,
            },
        ),
        (
            "startTime",
            PluginValue::Float(process.get_start_time().map_err(failed)?),
        ),
    ];
    Ok(PluginValue::Array(
        facts
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
    ))
}

fn dispatch_filesystem_method(
    fs: &std::rc::Rc<std::cell::RefCell<crate::util::Filesystem>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let path_arg = |position: usize| {
        arg::<BString>(method_name, args, position)
            .map(|path| std::path::PathBuf::from(path_from_bytes(&path)))
    };
    let failed = |error: anyhow::Error| error_throw(bconcat!(method_name, " failed"), &error);
    match method_name.as_bytes() {
        b"remove" => Ok(fs
            .borrow_mut()
            .remove(path_arg(0)?)
            .map_err(failed)?
            .to_plugin_value()),
        b"isDirEmpty" => Ok(fs.borrow().is_dir_empty(&path_arg(0)?).to_plugin_value()),
        b"emptyDirectory" => {
            fs.borrow_mut()
                .empty_directory(&path_arg(0)?, arg::<bool>(method_name, args, 1)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        b"removeDirectory" => Ok(fs
            .borrow_mut()
            .remove_directory(path_arg(0)?)
            .map_err(failed)?
            .to_plugin_value()),
        b"removeDirectoryAsync" => {
            let directory = path_arg(0)?;
            let executor = fs.borrow_mut().get_process_handle();
            let fs = fs.clone();
            let mut future: std::pin::Pin<
                Box<dyn std::future::Future<Output = anyhow::Result<bool>>>,
            > = Box::pin(async move {
                crate::util::Filesystem::remove_directory_async_via(&fs, &directory).await
            });
            // PHP settles the edge cases and starts the removal process before the call returns,
            // and raises what goes wrong up to there out of the call.
            match poll_once(&mut future) {
                std::task::Poll::Ready(removed) => {
                    resolved_promise(removed.map_err(failed)?.to_plugin_value())
                }
                std::task::Poll::Pending => Ok(job_registry::register(
                    JobOwner::process_executor(&executor),
                    executor,
                    std::rc::Rc::new(std::cell::Cell::new(true)),
                    runtime_throw("Aborted process"),
                    Box::pin(async move {
                        future
                            .await
                            .map(|removed| removed.to_plugin_value())
                            .map_err(|error| error_throw(b"removeDirectoryAsync failed", &error))
                    }),
                )),
            }
        }
        b"removeDirectoryPhp" => Ok(fs
            .borrow_mut()
            .remove_directory_php(&path_arg(0)?)
            .map_err(failed)?
            .to_plugin_value()),
        b"ensureDirectoryExists" => {
            fs.borrow_mut()
                .ensure_directory_exists(&path_arg(0)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        b"unlink" => Ok(fs
            .borrow()
            .unlink(path_arg(0)?)
            .map_err(failed)?
            .to_plugin_value()),
        b"rmdir" => Ok(fs
            .borrow()
            .rmdir(path_arg(0)?)
            .map_err(failed)?
            .to_plugin_value()),
        b"copyThenRemove" => {
            fs.borrow_mut()
                .copy_then_remove(&path_arg(0)?, &path_arg(1)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        b"copy" => Ok(fs
            .borrow_mut()
            .copy(&path_arg(0)?, &path_arg(1)?)
            .map_err(failed)?
            .to_plugin_value()),
        b"rename" => {
            fs.borrow_mut()
                .rename(path_arg(0)?, path_arg(1)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        b"findShortestPath" | b"findShortestPathCode" => {
            let from = path_arg(0)?;
            let to = path_arg(1)?;
            let fs = fs.borrow();
            Ok(if method_name == "findShortestPath" {
                fs.find_shortest_path(
                    &from,
                    &to,
                    arg::<bool>(method_name, args, 2)?,
                    arg::<bool>(method_name, args, 3)?,
                )
                .map_err(failed)?
                .to_plugin_value()
            } else {
                fs.find_shortest_path_code(
                    &from,
                    &to,
                    arg::<bool>(method_name, args, 2)?,
                    arg::<bool>(method_name, args, 3)?,
                    arg::<bool>(method_name, args, 4)?,
                )
                .map_err(failed)?
                .to_plugin_value()
            })
        }
        b"isAbsolutePath" => Ok(fs
            .borrow()
            .is_absolute_path(&path_arg(0)?)
            .to_plugin_value()),
        b"size" => Ok(fs
            .borrow()
            .size(path_arg(0)?)
            .map_err(failed)?
            .to_plugin_value()),
        b"normalizePath" => Ok(fs.borrow().normalize_path(&path_arg(0)?).to_plugin_value()),
        b"relativeSymlink" => Ok(fs
            .borrow()
            .relative_symlink(&path_arg(0)?, &path_arg(1)?)
            .map_err(failed)?
            .to_plugin_value()),
        b"isSymlinkedDirectory" => Ok(fs
            .borrow()
            .is_symlinked_directory(&path_arg(0)?)
            .to_plugin_value()),
        b"junction" => {
            fs.borrow_mut()
                .junction(&path_arg(0)?, &path_arg(1)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        b"isJunction" => Ok(fs.borrow().is_junction(&path_arg(0)?).to_plugin_value()),
        b"removeJunction" => Ok(fs
            .borrow_mut()
            .remove_junction(&path_arg(0)?)
            .map_err(failed)?
            .to_plugin_value()),
        b"filePutContentsIfModified" => Ok(fs
            .borrow()
            .file_put_contents_if_modified(&path_arg(0)?, arg::<BString>(method_name, args, 1)?)
            .map_err(failed)?
            .to_plugin_value()),
        b"safeCopy" => {
            fs.borrow()
                .safe_copy(&path_arg(0)?, &path_arg(1)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        other => Err(runtime_throw(bconcat!(
            "the Filesystem method `",
            other,
            "` is not available over RPC yet"
        ))),
    }
}

/// Serves the `ProcessExecutor` proxy stub, whether the executor behind it belongs to the object
/// graph or was built by plugin code writing `new ProcessExecutor(...)`. Both run their children in
/// the Rust process, which is what keeps the timeout, the executable path cache and the job budget
/// single-sourced across the boundary instead of forking a copy per world.
fn dispatch_process_executor_method(
    process: &std::rc::Rc<std::cell::RefCell<crate::util::ProcessExecutor>>,
    method_name: &BStr,
    args: &[PluginValue],
    out_params: &mut IndexMap<u32, PluginValue>,
) -> Result<PluginValue, PhpThrow> {
    let failed = |error: anyhow::Error| error_throw(bconcat!(method_name, " failed"), &error);
    let cwd_arg = |position: usize| -> Result<Option<std::path::PathBuf>, PhpThrow> {
        match args.get(position) {
            None | Some(PluginValue::Null) => Ok(None),
            _ => arg::<BString>(method_name, args, position)
                .map(|cwd| Some(std::path::PathBuf::from(path_from_bytes(&cwd)))),
        }
    };
    match method_name.as_bytes() {
        b"execute" => {
            let command = exec_command_arg(method_name, args, 0)?;
            let cwd = cwd_arg(2)?;
            // PHP selects the output handling with `func_num_args() > 1` and `is_callable($output)`;
            // the stub reproduces the caller's arity, so an absent argument is the forwarding case.
            let Some(output) = args.get(1) else {
                return process
                    .borrow()
                    .execute(
                        command,
                        crate::util::ProcessExecutor::FORWARD_OUTPUT,
                        cwd.as_deref(),
                    )
                    .map(|code| code.to_plugin_value())
                    .map_err(failed);
            };
            if is_php_callable(output).map_err(failed)? {
                let callable = output.clone();
                let callback: Box<dyn FnMut(&BStr, &BStr) -> anyhow::Result<bool>> =
                    Box::new(move |r#type: &BStr, buffer: &BStr| {
                        call_php_callable(
                            &callable,
                            vec![PluginValue::string(r#type), PluginValue::string(buffer)],
                        )?;
                        Ok(false)
                    });
                return process
                    .borrow()
                    .execute(command, callback, cwd.as_deref())
                    .map(|code| code.to_plugin_value())
                    .map_err(failed);
            }
            let mut captured: Option<BString> = None;
            let code = process
                .borrow()
                .execute(command, &mut captured, cwd.as_deref())
                .map_err(failed)?;
            // PHP leaves `$output` alone when the child was signaled before the assignment, which
            // is what an absent out-param position reproduces.
            if let Some(captured) = captured {
                out_params.insert(1, PluginValue::string(captured));
            }
            Ok(code.to_plugin_value())
        }
        b"executeTty" => Ok(process
            .borrow()
            .execute_tty(
                exec_command_arg(method_name, args, 0)?,
                cwd_arg(1)?.as_deref(),
            )
            .map_err(failed)?
            .to_plugin_value()),
        b"splitLines" => {
            let output = match args.first() {
                None | Some(PluginValue::Null) => BString::default(),
                _ => arg::<BString>(method_name, args, 0)?,
            };
            Ok(PluginValue::List(
                process
                    .borrow()
                    .split_lines(&output)
                    .into_iter()
                    .map(PluginValue::string)
                    .collect(),
            ))
        }
        b"getErrorOutput" => Ok(PluginValue::string(process.borrow().get_error_output())),
        b"requiresGitDirEnv" => Ok(process
            .borrow()
            .requires_git_dir_env(&exec_command_arg(method_name, args, 0)?)
            .to_plugin_value()),
        b"setMaxJobs" => {
            process
                .borrow()
                .set_max_jobs(arg::<i64>(method_name, args, 0)?);
            Ok(PluginValue::Null)
        }
        b"resetMaxJobs" => {
            process.borrow().reset_max_jobs();
            Ok(PluginValue::Null)
        }
        b"enableAsync" => {
            process.borrow().enable_async();
            Ok(PluginValue::Null)
        }
        b"executeAsync" => {
            let command = exec_command_arg(method_name, args, 0)?;
            let cwd = cwd_arg(1)?;
            let started = std::rc::Rc::new(std::cell::Cell::new(false));
            let mut future =
                process
                    .borrow()
                    .execute_async_job(command, cwd.as_deref(), Some(started.clone()));
            let resolve = |child: shirabe_symfony_process::Process| {
                rust_handle_value(
                    register_entity(RustEntity::Process(std::rc::Rc::new(
                        std::cell::RefCell::new(child),
                    ))),
                    BStr::new("Shirabe\\RustProcessStub"),
                )
            };
            let future: std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<PluginValue, PhpThrow>>>,
            > = match poll_once(&mut future) {
                // PHP raises the async gate out of `executeAsync()` itself, and it is the port's
                // only `LogicException` here; a child that fails to start rejects the promise.
                std::task::Poll::Ready(Err(error)) if error.is_instanceof::<LogicException>() => {
                    return Err(failed(error));
                }
                std::task::Poll::Ready(Err(error)) => return rejected_promise(failed(error)),
                // The child exited within the poll. PHP resolves only from `countActiveJobs()`,
                // so the result waits in the registry for the next tick.
                std::task::Poll::Ready(Ok(child)) => Box::pin(async move { Ok(resolve(child)) }),
                std::task::Poll::Pending => Box::pin(async move {
                    future
                        .await
                        .map(resolve)
                        .map_err(|error| error_throw(b"executeAsync failed", &error))
                }),
            };
            Ok(job_registry::register(
                JobOwner::process_executor(process),
                process.clone(),
                started,
                // TODO(async): cancelling drops the future and the `Process` in it, whose drop
                // stops the child with SIGTERM; PHP sends SIGINT first and then calls `stop(1)`.
                runtime_throw("Aborted process"),
                future,
            ))
        }
        b"wait" | b"countActiveJobs" => dispatch_tick_method(
            JobOwner::process_executor(process),
            PROCESS_EXECUTOR_TICK_CAP,
            method_name,
            args,
        ),
        other => Err(runtime_throw(bconcat!(
            "unknown ProcessExecutor method `",
            other,
            "`"
        ))),
    }
}

/// Serves the `HttpDownloader` proxy stub, whether the downloader behind it belongs to the object
/// graph or was built by plugin code writing `new HttpDownloader(...)`. Both run their requests
/// out of the Rust process, which is what keeps the authentication a run collects, the TLS
/// defaults and the parallel-request budget single-sourced across the boundary.
fn dispatch_http_downloader_method(
    downloader: &std::rc::Rc<std::cell::RefCell<crate::util::HttpDownloader>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getOptions" => Ok(PluginValue::from_php_mixed(&PhpMixed::Array(
            downloader.borrow().get_options().clone(),
        ))),
        b"setOptions" => {
            let options = arg::<IndexMap<BString, PhpMixed>>(method_name, args, 0)?;
            downloader.borrow_mut().set_options(options);
            Ok(PluginValue::Null)
        }
        b"get" => {
            let url = arg::<BString>(method_name, args, 0)?;
            let options =
                arg_or::<IndexMap<BString, PhpMixed>>(method_name, args, 1, IndexMap::new())?;
            let response = downloader
                .borrow()
                .get(&url, options)
                .map_err(|error| error_throw(b"get failed", &error))?;
            Ok(response_to_wire(&response))
        }
        b"copy" => {
            let url = arg::<BString>(method_name, args, 0)?;
            let to = arg::<BString>(method_name, args, 1)?;
            let options =
                arg_or::<IndexMap<BString, PhpMixed>>(method_name, args, 2, IndexMap::new())?;
            let response = downloader
                .borrow()
                .copy(&url, path_from_bytes(&to), options)
                .map_err(|error| error_throw(b"copy failed", &error))?;
            Ok(response_to_wire(&response))
        }
        b"add" | b"addCopy" => {
            let url = arg::<BString>(method_name, args, 0)?;
            let (to, options_position) = if method_name == "add" {
                (None, 1)
            } else {
                (Some(arg::<BString>(method_name, args, 1)?), 2)
            };
            let options = arg_or::<IndexMap<BString, PhpMixed>>(
                method_name,
                args,
                options_position,
                IndexMap::new(),
            )?;
            let failed =
                |error: anyhow::Error| error_throw(bconcat!(method_name, " failed"), &error);
            let started = std::rc::Rc::new(std::cell::Cell::new(false));
            // PHP raises the empty-url check and the async gate out of `add()`/`addCopy()`
            // itself, before the promise exists; only a failed request becomes a rejection.
            let job = downloader
                .borrow()
                .add_job(
                    url.as_bstr(),
                    to.as_ref().map(path_from_bytes),
                    options,
                    started.clone(),
                )
                .map_err(failed)?;
            match job {
                crate::util::http_downloader::HttpDownloaderJob::Settled(Ok(response)) => {
                    resolved_promise(response_to_wire(&response))
                }
                crate::util::http_downloader::HttpDownloaderJob::Settled(Err(error)) => {
                    rejected_promise(failed(error))
                }
                crate::util::http_downloader::HttpDownloaderJob::Pending(future) => {
                    let method = method_name.to_owned();
                    Ok(job_registry::register(
                        JobOwner::http_downloader(downloader),
                        downloader.clone(),
                        started,
                        error_throw(
                            BStr::new(""),
                            &crate::util::HttpDownloader::canceled(url.as_bstr()).into(),
                        ),
                        Box::pin(async move {
                            future
                                .await
                                .map(|response| response_to_wire(&response))
                                .map_err(|error| error_throw(bconcat!(method, " failed"), &error))
                        }),
                    ))
                }
            }
        }
        b"enableAsync" => {
            downloader.borrow_mut().enable_async();
            Ok(PluginValue::Null)
        }
        b"wait" | b"countActiveJobs" => dispatch_tick_method(
            JobOwner::http_downloader(downloader),
            HTTP_DOWNLOADER_TICK_CAP,
            method_name,
            args,
        ),
        other => Err(runtime_throw(bconcat!(
            "unknown HttpDownloader method `",
            other,
            "`"
        ))),
    }
}

/// Decodes the `string|non-empty-list<string>` a process executor takes as its command.
fn exec_command_arg(
    method: &BStr,
    args: &[PluginValue],
    position: usize,
) -> Result<crate::util::process_executor::CommandLine, PhpThrow> {
    match args.get(position) {
        Some(PluginValue::String(bytes)) => Ok(crate::util::process_executor::CommandLine::Shell(
            bytes.clone(),
        )),
        Some(PluginValue::List(items)) => {
            let mut parts = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                match item {
                    PluginValue::String(bytes) => {
                        parts.push(bytes.clone());
                    }
                    other => {
                        return Err(runtime_throw(bconcat!(
                            method,
                            " expects a list of strings at position ",
                            position,
                            ", \
                             but element ",
                            index,
                            " is ",
                            display_to_bytes(format_args!("{other:?}"))
                        )));
                    }
                }
            }
            Ok(crate::util::process_executor::CommandLine::Args(
                parts.into_iter().collect::<Vec<_>>(),
            ))
        }
        other => Err(arg_throw(
            method,
            position,
            "a command string or argument list",
            other,
        )),
    }
}

/// PHP's `is_callable($value)`, answered by the worker because only its own function and class
/// tables can say whether a string or a `[$object, 'method']` pair names something callable.
/// Values that cannot name a callable at all are answered here rather than over a round trip.
fn is_php_callable(value: &PluginValue) -> anyhow::Result<bool> {
    match value {
        PluginValue::String(_)
        | PluginValue::List(_)
        | PluginValue::Array(_)
        | PluginValue::PhpHandle(_)
        | PluginValue::PhpClass(_) => {
            let answer = unwrap_php_result(call_function_with_dispatcher(
                "is_callable",
                vec![value.clone()],
                Some(&mut PluginRpcDispatcher::default()),
            ))?;
            match answer {
                PluginValue::Bool(answer) => Ok(answer),
                other => Err(RuntimeException::new(bconcat!(
                    "is_callable did not return a bool over RPC: ",
                    display_to_bytes(format_args!("{other:?}"))
                ))
                .into()),
            }
        }
        _ => Ok(false),
    }
}

/// Calls a PHP callable value in the worker, whatever form it takes: `call_user_func` resolves a
/// closure, a function name and an `[$object, 'method']` pair exactly as the original call site
/// would have.
fn call_php_callable(
    callable: &PluginValue,
    args: Vec<PluginValue>,
) -> anyhow::Result<PluginValue> {
    let mut call_args = Vec::with_capacity(args.len() + 1);
    call_args.push(callable.clone());
    call_args.extend(args);
    unwrap_php_result(call_function_with_dispatcher(
        "call_user_func",
        call_args,
        Some(&mut PluginRpcDispatcher::default()),
    ))
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
            return Err(runtime_throw(bconcat!(
                "__shirabeCallStatic expects a class name, a method name and an argument list, \
                 got ",
                display_to_bytes(format_args!("{other:?}"))
            )));
        }
    };
    let name = bconcat!(class, "::", method);
    let string_arg = |position: usize| arg::<BString>(name.as_bstr(), &call_args, position);
    match (class.as_slice(), method.as_slice()) {
        (b"Composer\\Util\\Filesystem", b"isLocalPath") => {
            Ok(crate::util::Filesystem::is_local_path(&string_arg(0)?).to_plugin_value())
        }
        (b"Composer\\Util\\Filesystem", b"getPlatformPath") => Ok(PluginValue::string(
            path_to_bytes(&crate::util::Filesystem::get_platform_path(&string_arg(0)?)),
        )),
        (b"Composer\\Package\\Locker", b"getContentHash") => {
            crate::package::Locker::get_content_hash(string_arg(0)?)
                .map(PluginValue::string)
                .map_err(|error| error_throw(b"getContentHash failed over RPC", &error))
        }
        (b"Composer\\Util\\ProcessExecutor", b"getTimeout") => {
            Ok(crate::util::ProcessExecutor::get_timeout().to_plugin_value())
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
        _ => Err(runtime_throw(bconcat!(
            "Shirabe does not support calling ",
            name,
            " from the plugin process yet"
        ))),
    }
}

fn dispatch_event_dispatcher_method(
    dispatcher: &std::rc::Rc<
        std::cell::RefCell<dyn crate::event_dispatcher::EventDispatcherInterface>,
    >,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"dispatch" => {
            let name = arg::<BString>(method_name, args, 0)?;
            let probe = crate::event_dispatcher::Event::from_name(name.clone());
            if dispatcher.borrow_mut().has_event_listeners(&probe) {
                // TODO(plugin): dispatching a worker-constructed event through the Rust-side
                // dispatcher needs the event object (and the console input it carries) proxied
                // back into this process, so only the no-listener case is answered here.
                return Err(runtime_throw(bconcat!(
                    "dispatching `",
                    name,
                    "` from the plugin process is not supported yet while listeners are registered for it"
                )));
            }
            // TODO(plugin): answering 0 here skips what `do_dispatch` does before it reaches the
            // listener loop, and upstream does both regardless of the listener count: the
            // `COMPOSER_DEBUG_EVENTS` trace line, and `push_event`'s circular-call detection
            // (a nested dispatch of the same event name throws there even with no listeners).
            Ok(PluginValue::Int(0))
        }
        other => Err(runtime_throw(bconcat!(
            "the EventDispatcher method `",
            other,
            "` is not available over RPC yet"
        ))),
    }
}

fn dispatch_locker_method(
    locker: &std::rc::Rc<std::cell::RefCell<dyn LockerInterface>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let failed =
        |error: anyhow::Error| error_throw(bconcat!(method_name, " failed over RPC"), &error);
    let with_dev_reqs = || arg_or::<bool>(method_name, args, 0, false);
    match method_name.as_bytes() {
        b"isLocked" => Ok(locker.borrow_mut().is_locked().to_plugin_value()),
        b"isFresh" => Ok(locker
            .borrow_mut()
            .is_fresh()
            .map_err(failed)?
            .to_plugin_value()),
        b"getLockedRepository" => {
            let repository = locker
                .borrow_mut()
                .get_locked_repository(with_dev_reqs()?)
                .map_err(failed)?;
            repository_handle_value(&repository.into())
        }
        b"getDevPackageNames" => Ok(locker
            .borrow_mut()
            .get_dev_package_names()
            .map_err(failed)?
            .to_plugin_value()),
        b"getPlatformRequirements" => Ok(locker
            .borrow_mut()
            .get_platform_requirements(with_dev_reqs()?)
            .map_err(failed)?
            .to_plugin_value()),
        b"getMinimumStability" => Ok(locker
            .borrow_mut()
            .get_minimum_stability()
            .map_err(failed)?
            .to_plugin_value()),
        b"getStabilityFlags" => Ok(locker
            .borrow_mut()
            .get_stability_flags()
            .map_err(failed)?
            .to_plugin_value()),
        b"getPreferStable" => Ok(locker
            .borrow_mut()
            .get_prefer_stable()
            .map_err(failed)?
            .to_plugin_value()),
        b"getPreferLowest" => Ok(locker
            .borrow_mut()
            .get_prefer_lowest()
            .map_err(failed)?
            .to_plugin_value()),
        b"getPlatformOverrides" => Ok(locker
            .borrow_mut()
            .get_platform_overrides()
            .map_err(failed)?
            .to_plugin_value()),
        b"getAliases" => Ok(locker
            .borrow_mut()
            .get_aliases()
            .map_err(failed)?
            .to_plugin_value()),
        b"getPluginApi" => Ok(locker
            .borrow_mut()
            .get_plugin_api()
            .map_err(failed)?
            .to_plugin_value()),
        b"getLockData" => Ok(locker
            .borrow_mut()
            .get_lock_data()
            .map_err(failed)?
            .to_plugin_value()),
        b"setLockData" => {
            let written = locker
                .borrow_mut()
                .set_lock_data(
                    arg::<Vec<PackageInterfaceHandle>>(method_name, args, 0)?,
                    arg::<Option<Vec<PackageInterfaceHandle>>>(method_name, args, 1)?,
                    arg::<IndexMap<BString, BString>>(method_name, args, 2)?,
                    arg::<IndexMap<BString, BString>>(method_name, args, 3)?,
                    arg::<Vec<IndexMap<BString, PhpMixed>>>(method_name, args, 4)?,
                    arg::<BString>(method_name, args, 5)?.as_ref(),
                    arg::<IndexMap<BString, i64>>(method_name, args, 6)?,
                    arg::<bool>(method_name, args, 7)?,
                    arg::<bool>(method_name, args, 8)?,
                    arg::<IndexMap<BString, PhpMixed>>(method_name, args, 9)?,
                    arg_or::<bool>(method_name, args, 10, true)?,
                )
                .map_err(failed)?;
            Ok(written.to_plugin_value())
        }
        b"getMissingRequirementInfo" => {
            let package = root_package_arg(method_name, args, 0)?;
            let include_dev = arg::<bool>(method_name, args, 1)?;
            Ok(locker
                .borrow_mut()
                .get_missing_requirement_info(package, include_dev)
                .map_err(failed)?
                .to_plugin_value())
        }
        // TODO(plugin): `JsonFile` has no proxy stub, so the lock file cannot be handed out and
        // no plugin holds a JsonFile it could pass to `updateHash()`.
        b"getJsonFile" | b"updateHash" => Err(runtime_throw(bconcat!(
            "the Locker method `",
            method_name,
            "` is not available over RPC yet: JsonFile cannot cross the boundary"
        ))),
        other => Err(runtime_throw(bconcat!(
            "the Locker method `",
            other,
            "` is not available over RPC yet"
        ))),
    }
}

fn dispatch_repository_manager_method(
    rm: &std::rc::Rc<std::cell::RefCell<dyn RepositoryManagerInterface>>,
    method_name: &BStr,
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getLocalRepository" => repository_handle_value(&rm.borrow().get_local_repository()),
        // TODO(plugin): the remaining RepositoryManager surface is widened on demand, driven
        // by explicit errors from real plugins.
        other => Err(runtime_throw(bconcat!(
            "the RepositoryManager method `",
            other,
            "` is not available over RPC yet"
        ))),
    }
}

fn dispatch_repository_method(
    repository: &RepositoryInterfaceHandle,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"hasPackage" => {
            let package = arg::<PackageInterfaceHandle>(method_name, args, 0)?;
            let has = repository
                .has_package(package)
                .map_err(|error| error_throw(b"hasPackage failed over RPC", &error))?;
            Ok(has.to_plugin_value())
        }
        b"addPackage" | b"removePackage" => {
            let package = arg::<PackageInterfaceHandle>(method_name, args, 0)?;
            let mut borrowed = repository.borrow_mut();
            let writable = borrowed
                .as_writable_repository_interface_mut()
                .ok_or_else(|| {
                    runtime_throw(bconcat!(
                        "the repository behind this handle is not writable, so `",
                        method_name,
                        "` cannot be called on it"
                    ))
                })?;
            let outcome = if method_name == "addPackage" {
                writable.add_package(package)
            } else {
                writable.remove_package(package)
            };
            outcome
                .map_err(|error| error_throw(bconcat!(method_name, " failed over RPC"), &error))?;
            Ok(PluginValue::Null)
        }
        b"getPackages" => {
            let packages = repository
                .borrow_mut()
                .get_packages()
                .map_err(|error| error_throw(b"getPackages failed over RPC", &error))?;
            let mut items = Vec::with_capacity(packages.len());
            for package in packages {
                items.push(package_handle_value(package.as_rc()));
            }
            Ok(PluginValue::List(items))
        }
        b"getCanonicalPackages" => {
            let mut borrowed = repository.borrow_mut();
            let writable = borrowed
                .as_writable_repository_interface_mut()
                .ok_or_else(|| {
                    runtime_throw("the repository behind this handle is not writable, so `getCanonicalPackages` cannot be called on it")
                })?;
            let packages = writable
                .get_canonical_packages()
                .map_err(|error| error_throw(b"getCanonicalPackages failed over RPC", &error))?;
            Ok(packages.to_plugin_value())
        }
        // TODO(plugin): the remaining RepositoryInterface surface is widened on demand,
        // driven by explicit errors from real plugins.
        other => Err(runtime_throw(bconcat!(
            "the repository method `",
            other,
            "` is not available over RPC yet"
        ))),
    }
}

/// Decodes one positional argument of an RPC method call. A missing argument decodes the same
/// way PHP's `null` does; a parameter's own default belongs to `arg_or`.
trait FromPluginArg: Sized {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow>;
}

/// Encodes a Rust value as the PHP value the calling proxy stub declares.
pub(crate) trait ToPluginValue {
    fn to_plugin_value(self) -> PluginValue;
}

/// Decodes the argument at `position`.
fn arg<T: FromPluginArg>(
    method: &BStr,
    args: &[PluginValue],
    position: usize,
) -> Result<T, PhpThrow> {
    T::from_arg(method, position, args.get(position))
}

/// Decodes the argument at `position` for a parameter whose PHP declaration carries a default:
/// an omitted argument, and the `null` PHP passes in its place, take `default`.
fn arg_or<T: FromPluginArg>(
    method: &BStr,
    args: &[PluginValue],
    position: usize,
    default: T,
) -> Result<T, PhpThrow> {
    match args.get(position) {
        None | Some(PluginValue::Null) => Ok(default),
        value => T::from_arg(method, position, value),
    }
}

fn arg_throw(
    method: &BStr,
    position: usize,
    expected: impl AsRef<[u8]>,
    value: Option<&PluginValue>,
) -> PhpThrow {
    runtime_throw(bconcat!(
        method,
        " expects ",
        expected.as_ref(),
        " at position ",
        position,
        ", got ",
        display_to_bytes(format_args!("{value:?}"))
    ))
}

impl FromPluginArg for bool {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(PluginValue::Bool(value)) => Ok(*value),
            other => Err(arg_throw(method, position, "a bool", other)),
        }
    }
}

impl FromPluginArg for i64 {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(PluginValue::Int(value)) => Ok(*value),
            other => Err(arg_throw(method, position, "an int", other)),
        }
    }
}

impl FromPluginArg for bstr::BString {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(PluginValue::String(bytes)) => Ok(bytes.clone()),
            other => Err(arg_throw(method, position, "a string", other)),
        }
    }
}

impl FromPluginArg for PhpMixed {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            None => Ok(PhpMixed::Null),
            Some(value) => value.to_php_mixed().map_err(|error| {
                runtime_throw(bconcat!(
                    method,
                    " could not decode its argument at position ",
                    position,
                    ": ",
                    error_chain_message(&error)
                ))
            }),
        }
    }
}

impl<T: FromPluginArg> FromPluginArg for Option<T> {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            None | Some(PluginValue::Null) => Ok(None),
            value => Ok(Some(T::from_arg(method, position, value)?)),
        }
    }
}

/// A PHP array argument as a list, accepting the keyed shape PHP allows anywhere a list is
/// documented.
impl<T: FromPluginArg> FromPluginArg for Vec<T> {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        let items: Vec<&PluginValue> = match value {
            Some(PluginValue::List(items)) => items.iter().collect(),
            Some(PluginValue::Array(map)) => map.values().collect(),
            other => return Err(arg_throw(method, position, "an array", other)),
        };
        items
            .into_iter()
            .map(|item| T::from_arg(method, position, Some(item)))
            .collect()
    }
}

/// A PHP array argument as a map. An empty PHP array is indistinguishable from an empty list on
/// the wire, so it decodes here as an empty map; a non-empty list decodes under its int keys.
impl<T: FromPluginArg> FromPluginArg for IndexMap<BString, T> {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        let entries: Vec<(BString, &PluginValue)> = match value {
            Some(PluginValue::Array(map)) => {
                map.iter().map(|(key, item)| (key.clone(), item)).collect()
            }
            Some(PluginValue::List(items)) => items
                .iter()
                .enumerate()
                .map(|(index, item)| (bconcat!(index), item))
                .collect(),
            other => return Err(arg_throw(method, position, "an array", other)),
        };
        entries
            .into_iter()
            .map(|(key, item)| Ok((key, T::from_arg(method, position, Some(item))?)))
            .collect()
    }
}

impl FromPluginArg for crate::package::Link {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(value) => link_from_wire(value),
            other => Err(arg_throw(method, position, "a Link", other)),
        }
    }
}

impl FromPluginArg for crate::package::Mirror {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        let row = match value {
            Some(PluginValue::Array(row)) => row,
            other => return Err(arg_throw(method, position, "a mirror map", other)),
        };
        let url = match row.get(b"url".as_slice()) {
            Some(PluginValue::String(url)) => url.clone(),
            other => {
                return Err(runtime_throw(bconcat!(
                    method,
                    " expects a string `url` in every mirror, got ",
                    display_to_bytes(format_args!("{other:?}"))
                )));
            }
        };
        let preferred = matches!(
            row.get(b"preferred".as_slice()),
            Some(PluginValue::Bool(true))
        );
        Ok(crate::package::Mirror { url, preferred })
    }
}

/// Resolves a package argument back to the Rust-side entity its proxy stub stands for.
impl FromPluginArg for PackageInterfaceHandle {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(PluginValue::RustHandle(handle)) => {
                match R_TABLE.with(|table| table.borrow().get(&handle.rhandle).cloned()) {
                    Some(RustEntity::Package(package)) => {
                        Ok(PackageInterfaceHandle::from_rc_unchecked(package))
                    }
                    _ => Err(runtime_throw(bconcat!(
                        method,
                        " expects a package handle, got Rust handle ",
                        handle.rhandle
                    ))),
                }
            }
            other => Err(arg_throw(method, position, "a package", other)),
        }
    }
}

/// Resolves an IO argument back to the Rust-side entity its proxy stub stands for.
impl FromPluginArg for std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(PluginValue::RustHandle(handle)) => {
                match R_TABLE.with(|table| table.borrow().get(&handle.rhandle).cloned()) {
                    Some(RustEntity::Io(io)) => Ok(io),
                    _ => Err(runtime_throw(bconcat!(
                        method,
                        " expects an IO handle, got Rust handle ",
                        handle.rhandle
                    ))),
                }
            }
            other => Err(arg_throw(method, position, "an IOInterface", other)),
        }
    }
}

/// Resolves a config argument back to the Rust-side entity its proxy stub stands for.
impl FromPluginArg for std::rc::Rc<std::cell::RefCell<crate::config::Config>> {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(PluginValue::RustHandle(handle)) => {
                match R_TABLE.with(|table| table.borrow().get(&handle.rhandle).cloned()) {
                    Some(RustEntity::Config(config)) => Ok(config),
                    _ => Err(runtime_throw(bconcat!(
                        method,
                        " expects a Config handle, got Rust handle ",
                        handle.rhandle
                    ))),
                }
            }
            other => Err(arg_throw(method, position, "a Config", other)),
        }
    }
}

/// Resolves an installation manager argument back to the Rust-side entity its proxy stub stands
/// for.
impl FromPluginArg for std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>> {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(PluginValue::RustHandle(handle)) => {
                match R_TABLE.with(|table| table.borrow().get(&handle.rhandle).cloned()) {
                    Some(RustEntity::InstallationManager(im)) => Ok(im),
                    _ => Err(runtime_throw(bconcat!(
                        method,
                        " expects an InstallationManager handle, got Rust handle ",
                        handle.rhandle
                    ))),
                }
            }
            other => Err(arg_throw(method, position, "an InstallationManager", other)),
        }
    }
}

impl FromPluginArg for std::rc::Rc<std::cell::RefCell<dyn LockerInterface>> {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(PluginValue::RustHandle(handle)) => {
                match R_TABLE.with(|table| table.borrow().get(&handle.rhandle).cloned()) {
                    Some(RustEntity::Locker(locker)) => Ok(locker),
                    _ => Err(runtime_throw(bconcat!(
                        method,
                        " expects a Locker handle, got Rust handle ",
                        handle.rhandle
                    ))),
                }
            }
            other => Err(arg_throw(method, position, "a Locker", other)),
        }
    }
}

/// Resolves a downloader argument back to the Rust-side entity its proxy stub stands for.
impl FromPluginArg for std::rc::Rc<std::cell::RefCell<crate::util::HttpDownloader>> {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(PluginValue::RustHandle(handle)) => {
                match R_TABLE.with(|table| table.borrow().get(&handle.rhandle).cloned()) {
                    Some(RustEntity::HttpDownloader(downloader)) => Ok(downloader),
                    _ => Err(runtime_throw(bconcat!(
                        method,
                        " expects an HttpDownloader handle, got Rust handle ",
                        handle.rhandle
                    ))),
                }
            }
            other => Err(arg_throw(method, position, "an HttpDownloader", other)),
        }
    }
}

/// Resolves a process executor argument back to the Rust-side entity its proxy stub stands for.
impl FromPluginArg for std::rc::Rc<std::cell::RefCell<crate::util::ProcessExecutor>> {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(PluginValue::RustHandle(handle)) => {
                match R_TABLE.with(|table| table.borrow().get(&handle.rhandle).cloned()) {
                    Some(RustEntity::ProcessExecutor(process)) => Ok(process),
                    _ => Err(runtime_throw(bconcat!(
                        method,
                        " expects a ProcessExecutor handle, got Rust handle ",
                        handle.rhandle
                    ))),
                }
            }
            other => Err(arg_throw(method, position, "a ProcessExecutor", other)),
        }
    }
}

/// Resolves a repository argument back to the Rust-side entity its proxy stub stands for.
impl FromPluginArg for RepositoryInterfaceHandle {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(PluginValue::RustHandle(handle)) => {
                match R_TABLE.with(|table| table.borrow().get(&handle.rhandle).cloned()) {
                    Some(RustEntity::Repository(repository)) => Ok(repository),
                    _ => Err(runtime_throw(bconcat!(
                        method,
                        " expects a repository handle, got Rust handle ",
                        handle.rhandle
                    ))),
                }
            }
            other => Err(arg_throw(method, position, "a repository", other)),
        }
    }
}

impl FromPluginArg for chrono::DateTime<chrono::Utc> {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(value) => date_time_from_wire(value),
            other => Err(arg_throw(method, position, "a date", other)),
        }
    }
}

impl FromPluginArg for PhpObjHandle {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(PluginValue::PhpHandle(handle)) => Ok(handle.clone()),
            other => Err(arg_throw(method, position, "a PHP object", other)),
        }
    }
}

impl FromPluginArg for DisplayMode {
    fn from_arg(
        method: &BStr,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(PluginValue::Int(0)) => Ok(DisplayMode::SourceRefIfDev),
            Some(PluginValue::Int(1)) => Ok(DisplayMode::SourceRef),
            Some(PluginValue::Int(2)) => Ok(DisplayMode::DistRef),
            other => Err(arg_throw(
                method,
                position,
                "a display mode of 0..=2",
                other,
            )),
        }
    }
}

impl ToPluginValue for PluginValue {
    fn to_plugin_value(self) -> PluginValue {
        self
    }
}

impl ToPluginValue for bool {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::Bool(self)
    }
}

impl ToPluginValue for i64 {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::Int(self)
    }
}

impl ToPluginValue for BString {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::String(self)
    }
}

impl ToPluginValue for &bstr::BStr {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::String(BString::from(self))
    }
}

impl ToPluginValue for std::path::PathBuf {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::string(path_to_bytes(&self))
    }
}

impl ToPluginValue for &std::path::Path {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::string(path_to_bytes(self))
    }
}

impl ToPluginValue for PhpMixed {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::from_php_mixed(&self)
    }
}

impl<T: ToPluginValue> ToPluginValue for Option<T> {
    fn to_plugin_value(self) -> PluginValue {
        match self {
            Some(value) => value.to_plugin_value(),
            None => PluginValue::Null,
        }
    }
}

impl<T: ToPluginValue> ToPluginValue for Vec<T> {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::List(self.into_iter().map(T::to_plugin_value).collect())
    }
}

/// An `array<string, T>` as PHP shapes it: an empty map crosses as a list, since an empty PHP
/// array is indistinguishable from an empty list on the wire.
impl<K: Into<BString>, T: ToPluginValue> ToPluginValue for IndexMap<K, T> {
    fn to_plugin_value(self) -> PluginValue {
        if self.is_empty() {
            PluginValue::List(Vec::new())
        } else {
            PluginValue::Array(
                self.into_iter()
                    .map(|(key, value)| (key.into(), value.to_plugin_value()))
                    .collect(),
            )
        }
    }
}

/// `array{url: string, preferred: bool}` as PHP shapes it.
impl ToPluginValue for crate::package::Mirror {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::Array(IndexMap::from([
            ("url".into(), PluginValue::string(self.url)),
            ("preferred".into(), PluginValue::Bool(self.preferred)),
        ]))
    }
}

/// The child rebuilds a link as a real `Composer\Package\Link`, constraint included.
///
/// TODO(plugin): links have no entity to intern against, so two calls of the same getter answer
/// with distinct child-side objects where upstream returns the identical one.
impl ToPluginValue for crate::package::Link {
    fn to_plugin_value(self) -> PluginValue {
        link_to_wire(&self)
    }
}

impl ToPluginValue for PackageInterfaceHandle {
    fn to_plugin_value(self) -> PluginValue {
        package_handle_value(self.as_rc())
    }
}

impl ToPluginValue for chrono::DateTime<chrono::Utc> {
    fn to_plugin_value(self) -> PluginValue {
        date_time_to_wire(&self)
    }
}

/// The read half of the `CompletePackage` / `RootPackage` surface, which `PackageInterface`
/// does not carry. `None` means the method belongs to the base surface below.
fn dispatch_complete_package_getter(
    package: &AnyPackage,
    method_name: &BStr,
) -> Result<Option<PluginValue>, PhpThrow> {
    let unavailable = || {
        runtime_throw(bconcat!(
            "`",
            method_name,
            "` is not available on this package over RPC"
        ))
    };
    let value = match method_name.as_bytes() {
        b"getScripts"
        | b"getRepositories"
        | b"getLicense"
        | b"getKeywords"
        | b"getDescription"
        | b"getHomepage"
        | b"getAuthors"
        | b"getSupport"
        | b"getFunding"
        | b"isAbandoned"
        | b"getReplacementPackage"
        | b"getArchiveName"
        | b"getArchiveExcludes" => {
            let package = package
                .as_complete_package_interface()
                .ok_or_else(unavailable)?;
            match method_name.as_bytes() {
                b"getScripts" => package.get_scripts().to_plugin_value(),
                b"getRepositories" => package.get_repositories().to_plugin_value(),
                b"getLicense" => package.get_license().to_plugin_value(),
                b"getKeywords" => package.get_keywords().to_plugin_value(),
                b"getDescription" => package.get_description().to_plugin_value(),
                b"getHomepage" => package.get_homepage().to_plugin_value(),
                b"getAuthors" => package.get_authors().to_plugin_value(),
                b"getSupport" => package.get_support().to_plugin_value(),
                b"getFunding" => package.get_funding().to_plugin_value(),
                b"isAbandoned" => PluginValue::Bool(package.is_abandoned()),
                b"getReplacementPackage" => package.get_replacement_package().to_plugin_value(),
                b"getArchiveName" => package.get_archive_name().to_plugin_value(),
                _ => package.get_archive_excludes().to_plugin_value(),
            }
        }
        b"getAliases"
        | b"getMinimumStability"
        | b"getStabilityFlags"
        | b"getReferences"
        | b"getPreferStable"
        | b"getConfig" => {
            let package = package
                .as_root_package_interface()
                .ok_or_else(unavailable)?;
            match method_name.as_bytes() {
                b"getAliases" => package.get_aliases().to_plugin_value(),
                b"getMinimumStability" => PluginValue::string(package.get_minimum_stability()),
                b"getStabilityFlags" => package.get_stability_flags().to_plugin_value(),
                b"getReferences" => package.get_references().to_plugin_value(),
                b"getPreferStable" => PluginValue::Bool(package.get_prefer_stable()),
                _ => package.get_config().to_plugin_value(),
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}

fn dispatch_package_method(
    package: &std::rc::Rc<std::cell::RefCell<AnyPackage>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"setId" => {
            let id = arg::<i64>(method_name, args, 0)?;
            package.borrow_mut().as_package_interface_mut().set_id(id);
            return Ok(PluginValue::Null);
        }
        b"setInstallationSource"
        | b"setSourceReference"
        | b"setSourceUrl"
        | b"setDistUrl"
        | b"setDistType"
        | b"setDistReference"
        | b"setSourceDistReferences" => {
            let value = arg::<Option<BString>>(method_name, args, 0)?;
            let mut borrowed = package.borrow_mut();
            let package = borrowed.as_package_interface_mut();
            match method_name.as_bytes() {
                b"setInstallationSource" => package.set_installation_source(value),
                b"setSourceReference" => package.set_source_reference(value),
                b"setSourceUrl" => package.set_source_url(value),
                b"setDistUrl" => package.set_dist_url(value),
                b"setDistType" => package.set_dist_type(value),
                b"setDistReference" => package.set_dist_reference(value),
                _ => package.set_source_dist_references(value.ok_or_else(|| {
                    runtime_throw("setSourceDistReferences expects a reference string")
                })?),
            }
            return Ok(PluginValue::Null);
        }
        b"setSourceMirrors" | b"setDistMirrors" => {
            let mirrors = arg::<Option<Vec<crate::package::Mirror>>>(method_name, args, 0)?;
            let mut borrowed = package.borrow_mut();
            let package = borrowed.as_package_interface_mut();
            if method_name == "setSourceMirrors" {
                package.set_source_mirrors(mirrors);
            } else {
                package.set_dist_mirrors(mirrors);
            }
            return Ok(PluginValue::Null);
        }
        b"setRepository" => {
            let repository = arg::<RepositoryInterfaceHandle>(method_name, args, 0)?;
            package
                .borrow_mut()
                .as_package_interface_mut()
                .set_repository(repository)
                .map_err(|error| error_throw(b"setRepository failed", &error))?;
            return Ok(PluginValue::Null);
        }
        b"setTransportOptions" => {
            let options =
                arg_or::<IndexMap<BString, PhpMixed>>(method_name, args, 0, IndexMap::new())?;
            package
                .borrow_mut()
                .as_package_interface_mut()
                .set_transport_options(options);
            return Ok(PluginValue::Null);
        }
        // `RootAliasPackage` overrides each of these to write through to its alias target, and
        // `RootPackage` reaches the same base state either way, so both go through the interface.
        //
        // TODO(type-model): choosing the body for the concrete variant belongs on `AnyPackage`,
        // not here. The stub surface already decides which classes carry a method, so the
        // `as_*_mut` accessors' "not available on an alias package" arms are unreachable for a
        // method the alias stubs do not declare, and what is left is a per-variant dispatch that
        // this guard only approximates: it covers `RootPackage` as well, where the extra hop is
        // equivalent only while that impl keeps delegating to the base package, and nothing
        // checks it. Method names repeated in the base-package arm below make the answer depend
        // on arm order, and a new variant compiles into the wrong body without a diagnostic.
        b"setRequires" | b"setDevRequires" | b"setConflicts" | b"setProvides" | b"setReplaces"
        | b"setAutoload" | b"setDevAutoload" | b"setSuggests" | b"setExtra"
            if package.borrow().is_root() =>
        {
            let mut borrowed = package.borrow_mut();
            let package = borrowed
                .as_root_package_interface_mut()
                .expect("a root package exposes RootPackageInterface");
            match method_name.as_bytes() {
                b"setRequires" => package.set_requires(arg_or::<
                    IndexMap<BString, crate::package::Link>,
                >(
                    method_name, args, 0, IndexMap::new()
                )?),
                b"setDevRequires" => {
                    package.set_dev_requires(arg_or::<IndexMap<BString, crate::package::Link>>(
                        method_name,
                        args,
                        0,
                        IndexMap::new(),
                    )?)
                }
                b"setConflicts" => package.set_conflicts(arg_or::<
                    IndexMap<BString, crate::package::Link>,
                >(
                    method_name, args, 0, IndexMap::new()
                )?),
                b"setProvides" => package.set_provides(arg_or::<
                    IndexMap<BString, crate::package::Link>,
                >(
                    method_name, args, 0, IndexMap::new()
                )?),
                b"setReplaces" => package.set_replaces(arg_or::<
                    IndexMap<BString, crate::package::Link>,
                >(
                    method_name, args, 0, IndexMap::new()
                )?),
                b"setAutoload" => {
                    package.set_autoload(arg::<IndexMap<BString, PhpMixed>>(method_name, args, 0)?)
                }
                b"setDevAutoload" => package.set_dev_autoload(arg::<IndexMap<BString, PhpMixed>>(
                    method_name,
                    args,
                    0,
                )?),
                b"setSuggests" => {
                    package.set_suggests(arg::<IndexMap<BString, BString>>(method_name, args, 0)?)
                }
                _ => package.set_extra(arg::<IndexMap<BString, PhpMixed>>(method_name, args, 0)?),
            }
            return Ok(PluginValue::Null);
        }
        // `Package`'s own setters. The concrete subclasses inherit them (their PHP overrides in
        // `RootPackage` delegate to the same base state), so the base package answers for every
        // real variant.
        b"setType"
        | b"setTargetDir"
        | b"setExtra"
        | b"setBinaries"
        | b"setSourceType"
        | b"setDistSha1Checksum"
        | b"setSuggests"
        | b"setAutoload"
        | b"setDevAutoload"
        | b"setIncludePaths"
        | b"setPhpExt"
        | b"setNotificationUrl"
        | b"setIsDefaultBranch"
        | b"replaceVersion" => {
            let mut borrowed = package.borrow_mut();
            let package = borrowed.as_package_mut().ok_or_else(|| {
                runtime_throw(bconcat!(
                    "`",
                    method_name,
                    "` is not available on an alias package over RPC"
                ))
            })?;
            match method_name.as_bytes() {
                b"setType" => package.set_type(arg::<BString>(method_name, args, 0)?),
                b"setTargetDir" => {
                    package.set_target_dir(arg::<Option<BString>>(method_name, args, 0)?)
                }
                b"setExtra" => {
                    package.set_extra(arg::<IndexMap<BString, PhpMixed>>(method_name, args, 0)?)
                }
                b"setBinaries" => package.set_binaries(arg::<Vec<BString>>(method_name, args, 0)?),
                b"setSourceType" => {
                    package.set_source_type(arg::<Option<BString>>(method_name, args, 0)?)
                }
                b"setDistSha1Checksum" => {
                    package.set_dist_sha1_checksum(arg::<Option<BString>>(method_name, args, 0)?)
                }
                b"setSuggests" => {
                    package.set_suggests(arg::<IndexMap<BString, BString>>(method_name, args, 0)?)
                }
                b"setAutoload" => {
                    package.set_autoload(arg::<IndexMap<BString, PhpMixed>>(method_name, args, 0)?)
                }
                b"setDevAutoload" => package.set_dev_autoload(arg::<IndexMap<BString, PhpMixed>>(
                    method_name,
                    args,
                    0,
                )?),
                b"setIncludePaths" => {
                    package.set_include_paths(arg::<Vec<BString>>(method_name, args, 0)?)
                }
                b"setPhpExt" => package.set_php_ext(arg::<Option<IndexMap<BString, PhpMixed>>>(
                    method_name,
                    args,
                    0,
                )?),
                b"setNotificationUrl" => {
                    package.set_notification_url(arg::<BString>(method_name, args, 0)?)
                }
                b"setIsDefaultBranch" => {
                    package.set_is_default_branch(arg::<bool>(method_name, args, 0)?)
                }
                _ => package.replace_version(
                    arg::<BString>(method_name, args, 0)?,
                    arg::<BString>(method_name, args, 1)?,
                ),
            }
            return Ok(PluginValue::Null);
        }
        b"setRequires" | b"setConflicts" | b"setProvides" | b"setReplaces" | b"setDevRequires" => {
            let links = arg_or::<IndexMap<BString, crate::package::Link>>(
                method_name,
                args,
                0,
                IndexMap::new(),
            )?;
            let mut borrowed = package.borrow_mut();
            let package = borrowed.as_package_mut().ok_or_else(|| {
                runtime_throw(bconcat!(
                    "`",
                    method_name,
                    "` is not available on an alias package over RPC"
                ))
            })?;
            match method_name.as_bytes() {
                b"setRequires" => package.set_requires(links),
                b"setConflicts" => package.set_conflicts(links),
                b"setProvides" => package.set_provides(links),
                b"setReplaces" => package.set_replaces(links),
                _ => package.set_dev_requires(links),
            }
            return Ok(PluginValue::Null);
        }
        b"setReleaseDate" => {
            let date = arg::<Option<chrono::DateTime<chrono::Utc>>>(method_name, args, 0)?;
            let mut borrowed = package.borrow_mut();
            let package = borrowed.as_package_mut().ok_or_else(|| {
                runtime_throw("`setReleaseDate` is not available on an alias package over RPC")
            })?;
            package.set_release_date(date);
            return Ok(PluginValue::Null);
        }
        b"setScripts"
        | b"setRepositories"
        | b"setLicense"
        | b"setKeywords"
        | b"setDescription"
        | b"setHomepage"
        | b"setAuthors"
        | b"setSupport"
        | b"setFunding"
        | b"setAbandoned"
        | b"setArchiveName"
        | b"setArchiveExcludes" => {
            let mut borrowed = package.borrow_mut();
            let package = borrowed
                .as_complete_package_interface_mut()
                .ok_or_else(|| {
                    runtime_throw(bconcat!(
                        "`",
                        method_name,
                        "` is not available on this package over RPC"
                    ))
                })?;
            match method_name.as_bytes() {
                b"setScripts" => package.set_scripts(arg::<IndexMap<BString, Vec<BString>>>(
                    method_name,
                    args,
                    0,
                )?),
                b"setRepositories" => package.set_repositories(arg::<IndexMap<BString, PhpMixed>>(
                    method_name,
                    args,
                    0,
                )?),
                b"setLicense" => package.set_license(arg::<Vec<BString>>(method_name, args, 0)?),
                b"setKeywords" => package.set_keywords(arg::<Vec<BString>>(method_name, args, 0)?),
                b"setDescription" => package.set_description(arg::<BString>(method_name, args, 0)?),
                b"setHomepage" => package.set_homepage(arg::<BString>(method_name, args, 0)?),
                b"setAuthors" => package.set_authors(arg::<Vec<IndexMap<BString, BString>>>(
                    method_name,
                    args,
                    0,
                )?),
                b"setSupport" => {
                    package.set_support(arg::<IndexMap<BString, BString>>(method_name, args, 0)?)
                }
                b"setFunding" => package.set_funding(arg::<Vec<IndexMap<BString, PhpMixed>>>(
                    method_name,
                    args,
                    0,
                )?),
                b"setAbandoned" => package.set_abandoned(arg::<PhpMixed>(method_name, args, 0)?),
                b"setArchiveName" => {
                    package.set_archive_name(arg::<BString>(method_name, args, 0)?)
                }
                _ => package.set_archive_excludes(arg::<Vec<BString>>(method_name, args, 0)?),
            }
            return Ok(PluginValue::Null);
        }
        b"setStabilityFlags"
        | b"setMinimumStability"
        | b"setPreferStable"
        | b"setConfig"
        | b"setReferences"
        | b"setAliases" => {
            let mut borrowed = package.borrow_mut();
            let package = borrowed.as_root_package_interface_mut().ok_or_else(|| {
                runtime_throw(bconcat!(
                    "`",
                    method_name,
                    "` is not available on this package over RPC"
                ))
            })?;
            match method_name.as_bytes() {
                b"setStabilityFlags" => package.set_stability_flags(arg::<IndexMap<BString, i64>>(
                    method_name,
                    args,
                    0,
                )?),
                b"setMinimumStability" => {
                    package.set_minimum_stability(arg::<BString>(method_name, args, 0)?)
                }
                b"setPreferStable" => package.set_prefer_stable(arg::<bool>(method_name, args, 0)?),
                b"setConfig" => {
                    package.set_config(arg::<IndexMap<BString, PhpMixed>>(method_name, args, 0)?)
                }
                b"setReferences" => {
                    package.set_references(arg::<IndexMap<BString, BString>>(method_name, args, 0)?)
                }
                _ => package.set_aliases(arg::<Vec<IndexMap<BString, BString>>>(
                    method_name,
                    args,
                    0,
                )?),
            }
            return Ok(PluginValue::Null);
        }
        b"equals" => {
            let other = arg::<PackageInterfaceHandle>(method_name, args, 0)?;
            let this = PackageInterfaceHandle::from_rc_unchecked(package.clone());
            return Ok(this.equals(&other).to_plugin_value());
        }
        // The subclasses narrow `getAliasOf`'s return type to their own alias target, but every
        // variant holds the one entity.
        b"getAliasOf" | b"isRootPackageAlias" | b"hasSelfVersionRequires" => {
            let borrowed = package.borrow();
            let alias = borrowed.as_alias_package().ok_or_else(|| {
                runtime_throw(bconcat!(
                    "`",
                    method_name,
                    "` is not available on this package over RPC"
                ))
            })?;
            return Ok(match method_name.as_bytes() {
                b"getAliasOf" => package_handle_value(alias.get_alias_of().as_rc()),
                b"isRootPackageAlias" => alias.is_root_package_alias().to_plugin_value(),
                _ => alias.has_self_version_requires().to_plugin_value(),
            });
        }
        b"setRootPackageAlias" => {
            let value = arg::<bool>(method_name, args, 0)?;
            package
                .borrow_mut()
                .as_alias_package_mut()
                .ok_or_else(|| {
                    runtime_throw("`setRootPackageAlias` is not available on this package over RPC")
                })?
                .set_root_package_alias(value);
            return Ok(PluginValue::Null);
        }
        _ => {}
    }

    if let Some(value) = dispatch_complete_package_getter(&package.borrow(), method_name)? {
        return Ok(value);
    }

    let borrowed = package.borrow();
    let package = borrowed.as_package_interface();
    match method_name.as_bytes() {
        b"getName" => Ok(package.get_name().to_plugin_value()),
        b"getPrettyName" => Ok(package.get_pretty_name().to_plugin_value()),
        b"getNames" => {
            let provides = arg_or::<bool>(method_name, args, 0, true)?;
            Ok(package.get_names(provides).to_plugin_value())
        }
        b"getId" => Ok(package.get_id().to_plugin_value()),
        b"isDev" => Ok(package.is_dev().to_plugin_value()),
        b"getType" => Ok(package.get_type().to_plugin_value()),
        b"getTargetDir" => Ok(package.get_target_dir().to_plugin_value()),
        b"getExtra" => Ok(package.get_extra().to_plugin_value()),
        b"getInstallationSource" => Ok(package.get_installation_source().to_plugin_value()),
        b"getSourceType" => Ok(package.get_source_type().to_plugin_value()),
        b"getSourceUrl" => Ok(package.get_source_url().to_plugin_value()),
        b"getSourceUrls" => Ok(package.get_source_urls().to_plugin_value()),
        b"getSourceReference" => Ok(package.get_source_reference().to_plugin_value()),
        b"getSourceMirrors" => Ok(package.get_source_mirrors().to_plugin_value()),
        b"getDistType" => Ok(package.get_dist_type().to_plugin_value()),
        b"getDistUrl" => Ok(package.get_dist_url().to_plugin_value()),
        b"getDistUrls" => Ok(package.get_dist_urls().to_plugin_value()),
        b"getDistReference" => Ok(package.get_dist_reference().to_plugin_value()),
        b"getDistSha1Checksum" => Ok(package.get_dist_sha1_checksum().to_plugin_value()),
        b"getDistMirrors" => Ok(package.get_dist_mirrors().to_plugin_value()),
        b"getVersion" => Ok(package.get_version().to_plugin_value()),
        b"getPrettyVersion" => Ok(package.get_pretty_version().to_plugin_value()),
        b"getFullPrettyVersion" => {
            let truncate = arg_or::<bool>(method_name, args, 0, true)?;
            let display_mode =
                arg_or::<DisplayMode>(method_name, args, 1, DisplayMode::SourceRefIfDev)?;
            Ok(package
                .get_full_pretty_version(truncate, display_mode)
                .to_plugin_value())
        }
        b"getStability" => Ok(package.get_stability().to_plugin_value()),
        b"getRequires" => Ok((*package.get_requires()).clone().to_plugin_value()),
        b"getConflicts" => Ok((*package.get_conflicts()).clone().to_plugin_value()),
        b"getProvides" => Ok((*package.get_provides()).clone().to_plugin_value()),
        b"getReplaces" => Ok((*package.get_replaces()).clone().to_plugin_value()),
        b"getDevRequires" => Ok((*package.get_dev_requires()).clone().to_plugin_value()),
        b"getSuggests" => Ok(package.get_suggests().to_plugin_value()),
        b"getAutoload" => Ok(package.get_autoload().to_plugin_value()),
        b"getDevAutoload" => Ok(package.get_dev_autoload().to_plugin_value()),
        b"getIncludePaths" => Ok(package.get_include_paths().to_plugin_value()),
        b"getPhpExt" => Ok(match package.get_php_ext() {
            Some(config) => config.to_plugin_value(),
            None => PluginValue::Null,
        }),
        b"getRepository" => match package.get_repository() {
            Some(repository) => repository_handle_value(&repository),
            None => Ok(PluginValue::Null),
        },
        b"getBinaries" => Ok(package.get_binaries().to_plugin_value()),
        b"getUniqueName" => Ok(package.get_unique_name().to_plugin_value()),
        b"getNotificationUrl" => Ok(package.get_notification_url().to_plugin_value()),
        b"__toString" => Ok(package.get_unique_name().to_plugin_value()),
        b"getPrettyString" => Ok(package.get_pretty_string().to_plugin_value()),
        b"isDefaultBranch" => Ok(package.is_default_branch().to_plugin_value()),
        // `BasePackage`'s concrete methods are not forwarded by `PackageInterface`, so both are
        // computed from the interface here, as `VersionSelector` already does for the second.
        b"isPlatform" => Ok(package
            .get_repository()
            .is_some_and(|repository| repository.is::<crate::repository::PlatformRepository>())
            .to_plugin_value()),
        b"getStabilityPriority" => Ok(crate::package::stability_priority(package.get_stability())
            .unwrap_or(crate::package::base_package::STABILITY_STABLE)
            .to_plugin_value()),
        b"getTransportOptions" => Ok(package.get_transport_options().to_plugin_value()),
        b"getReleaseDate" => Ok(package.get_release_date().to_plugin_value()),
        other => Err(runtime_throw(bconcat!(
            "the package method `",
            other,
            "` is not available over RPC yet"
        ))),
    }
}

fn dispatch_operation_method(
    operation: &AnyOperation,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match (method_name.as_bytes(), operation) {
        (b"getOperationType", _) => Ok(operation.get_operation_type().to_plugin_value()),
        (b"show", _) => Ok(operation
            .show(arg::<bool>(method_name, args, 0)?)
            .to_plugin_value()),
        (b"__toString", _) => Ok(operation.show(false).to_plugin_value()),
        (b"getPackage", AnyOperation::Install(op)) => {
            Ok(package_handle_value(op.get_package().as_rc()))
        }
        (b"getPackage", AnyOperation::Uninstall(op)) => {
            Ok(package_handle_value(op.get_package().as_rc()))
        }
        (b"getPackage", AnyOperation::MarkAliasInstalled(op)) => {
            Ok(package_handle_value(op.get_package().as_rc()))
        }
        (b"getPackage", AnyOperation::MarkAliasUninstalled(op)) => {
            Ok(package_handle_value(op.get_package().as_rc()))
        }
        (b"getInitialPackage", AnyOperation::Update(op)) => {
            Ok(package_handle_value(op.get_initial_package().as_rc()))
        }
        (b"getTargetPackage", AnyOperation::Update(op)) => {
            Ok(package_handle_value(op.get_target_package().as_rc()))
        }
        (other, _) => Err(runtime_throw(bconcat!(
            "the operation method `",
            other,
            "` is not available on a ",
            operation_stub_class(operation),
            " over RPC yet"
        ))),
    }
}

/// Serves the methods `PreFileDownloadEvent` declares beyond the base event's.
pub(crate) fn dispatch_pre_file_download_event_method(
    event: &mut PreFileDownloadEvent,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getHttpDownloader" => Ok(rust_handle_value(
            register_entity(RustEntity::HttpDownloader(
                event.get_http_downloader().clone(),
            )),
            BStr::new("Composer\\Util\\HttpDownloader"),
        )),
        b"getProcessedUrl" => Ok(event.get_processed_url().to_plugin_value()),
        b"setProcessedUrl" => {
            event.set_processed_url(arg::<BString>(method_name, args, 0)?);
            Ok(PluginValue::Null)
        }
        b"getCustomCacheKey" => Ok(event.get_custom_cache_key().to_plugin_value()),
        b"setCustomCacheKey" => {
            event.set_custom_cache_key(arg::<Option<BString>>(method_name, args, 0)?);
            Ok(PluginValue::Null)
        }
        b"getType" => Ok(event.get_type().to_plugin_value()),
        b"getContext" => file_download_context_value(event.get_context()),
        b"getTransportOptions" => Ok(event.get_transport_options().clone().to_plugin_value()),
        b"setTransportOptions" => {
            event.set_transport_options(arg::<IndexMap<BString, PhpMixed>>(method_name, args, 0)?);
            Ok(PluginValue::Null)
        }
        other => Err(runtime_throw(bconcat!(
            "the Event method `",
            other,
            "` is not available over RPC yet"
        ))),
    }
}

/// Serves the methods `PostFileDownloadEvent` declares beyond the base event's.
pub(crate) fn dispatch_post_file_download_event_method(
    event: &PostFileDownloadEvent,
    method_name: &BStr,
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getFileName" => Ok(event.get_file_name().to_plugin_value()),
        b"getChecksum" => Ok(event.get_checksum().to_plugin_value()),
        b"getUrl" => Ok(event.get_url().to_plugin_value()),
        b"getContext" => file_download_context_value(event.get_context()),
        b"getPackage" => Ok(event.get_package().to_plugin_value()),
        b"getType" => Ok(event.get_type().to_plugin_value()),
        other => Err(runtime_throw(bconcat!(
            "the Event method `",
            other,
            "` is not available over RPC yet"
        ))),
    }
}

/// Serves the methods `PrePoolCreateEvent` declares beyond the base event's.
pub(crate) fn dispatch_pre_pool_create_event_method(
    event: &mut PrePoolCreateEvent,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getRepositories" => Ok(PluginValue::List(
            event
                .get_repositories()
                .iter()
                .map(repository_handle_value)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        // TODO(plugin): Request has no proxy stub, so the request a listener would inspect or
        // change cannot cross the boundary yet.
        b"getRequest" => Err(runtime_throw(
            "the Request of a PrePoolCreateEvent is not available over RPC yet",
        )),
        b"getAcceptableStabilities" => {
            Ok(event.get_acceptable_stabilities().clone().to_plugin_value())
        }
        b"getStabilityFlags" => Ok(event.get_stability_flags().clone().to_plugin_value()),
        b"getRootAliases" => Ok(event.get_root_aliases().clone().to_plugin_value()),
        b"getRootReferences" => Ok(event.get_root_references().clone().to_plugin_value()),
        b"getPackages" => Ok(event.get_packages().clone().to_plugin_value()),
        b"getUnacceptableFixedPackages" => Ok(event
            .get_unacceptable_fixed_packages()
            .clone()
            .to_plugin_value()),
        b"setPackages" => {
            event.set_packages(arg::<Vec<PackageInterfaceHandle>>(method_name, args, 0)?);
            Ok(PluginValue::Null)
        }
        b"setUnacceptableFixedPackages" => {
            event.set_unacceptable_fixed_packages(arg::<Vec<PackageInterfaceHandle>>(
                method_name,
                args,
                0,
            )?);
            Ok(PluginValue::Null)
        }
        other => Err(runtime_throw(bconcat!(
            "the Event method `",
            other,
            "` is not available over RPC yet"
        ))),
    }
}

/// The `$context` of a file download event as the PHP value the event stub returns.
fn file_download_context_value(
    context: Option<&FileDownloadContext>,
) -> Result<PluginValue, PhpThrow> {
    match context {
        None => Ok(PluginValue::Null),
        Some(FileDownloadContext::Package(package)) => Ok(package.clone().to_plugin_value()),
        // TODO(plugin): the array a repository passes names the repository itself, which cannot
        // cross the boundary yet.
        Some(FileDownloadContext::Repository) => Err(runtime_throw(
            "the repository context of a file download event is not available over RPC yet",
        )),
    }
}

fn dispatch_transaction_method(
    transaction: &AnyTransaction,
    method_name: &BStr,
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getOperations" => Ok(PluginValue::List(
            transaction
                .get_operations()
                .iter()
                .map(operation_handle_value)
                .collect(),
        )),
        other => Err(runtime_throw(bconcat!(
            "the Transaction method `",
            other,
            "` is not available over RPC yet"
        ))),
    }
}

fn dispatch_installation_manager_method(
    im: &std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getInstallPath" => {
            let package = arg::<PackageInterfaceHandle>(method_name, args, 0)?;
            Ok(im.borrow().get_install_path(package).to_plugin_value())
        }
        b"addInstaller" | b"removeInstaller" => {
            let handle = arg::<PhpObjHandle>(method_name, args, 0)?;
            if !php_is_a(
                &handle,
                BStr::new("Composer\\Installer\\InstallerInterface"),
            )
            .map_err(|error| {
                runtime_throw(bconcat!(
                    method_name,
                    " could not type-check its argument: ",
                    error_chain_message(&error)
                ))
            })? {
                return Err(runtime_throw(bconcat!(
                    method_name,
                    " expects a Composer\\Installer\\InstallerInterface, got ",
                    handle.class
                )));
            }
            let phandle = handle.phandle;
            let installer = php_installer_proxy(handle);
            if method_name == "addInstaller" {
                im.borrow().add_installer(installer);
            } else {
                im.borrow().remove_installer(&*installer);
                forget_php_installer_proxy(phandle);
            }
            Ok(PluginValue::Null)
        }
        // TODO(plugin): the remaining InstallationManager surface is widened on demand,
        // driven by explicit errors from real plugins.
        other => Err(runtime_throw(bconcat!(
            "the InstallationManager method `",
            other,
            "` is not available over RPC yet"
        ))),
    }
}

fn dispatch_io_method(
    io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"write" | b"writeError" => {
            let (messages, newline, verbosity) = decode_write_args(method_name, args)?;
            let borrowed = io.borrow();
            for message in &messages {
                if method_name == "write" {
                    borrowed.write_bytes(message.as_bstr(), newline, verbosity);
                } else {
                    borrowed.write_error_bytes(message.as_bstr(), newline, verbosity);
                }
            }
            Ok(PluginValue::Null)
        }
        b"isInteractive" => Ok(io.borrow().is_interactive().to_plugin_value()),
        b"isVerbose" => Ok(io.borrow().is_verbose().to_plugin_value()),
        b"isVeryVerbose" => Ok(io.borrow().is_very_verbose().to_plugin_value()),
        b"isDebug" => Ok(io.borrow().is_debug().to_plugin_value()),
        b"isDecorated" => Ok(io.borrow().is_decorated().to_plugin_value()),
        // TODO(plugin): the remaining IOInterface surface (ask*, authentications, ...) is
        // widened on demand, driven by explicit errors from real plugins.
        other => Err(runtime_throw(bconcat!(
            "the IO method `",
            other,
            "` is not available over RPC yet"
        ))),
    }
}

/// Decodes `($messages, bool $newline, int $verbosity)`: `$messages` is a string or a list of
/// strings in PHP.
fn decode_write_args(
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<(Vec<BString>, bool, i64), PhpThrow> {
    let messages = match args.first() {
        Some(PluginValue::String(_)) => vec![arg::<BString>(method_name, args, 0)?],
        _ => arg::<Vec<BString>>(method_name, args, 0)?,
    };
    let newline = arg_or::<bool>(method_name, args, 1, true)?;
    let verbosity = arg_or::<i64>(method_name, args, 2, crate::io::NORMAL)?;
    Ok((messages, newline, verbosity))
}

fn runtime_throw(message: impl Into<BString>) -> PhpThrow {
    PhpThrow::runtime(message)
}

/// The `Throw` a failed Rust-side call crosses the boundary as. An error carrying a ported PHP
/// exception keeps that exception's class, code and declared state, so a plugin catches what it
/// would catch under Composer; one carrying no exception keeps the `RuntimeException` shape,
/// with `context` naming the call that failed.
fn error_throw(context: impl AsRef<[u8]>, error: &anyhow::Error) -> PhpThrow {
    match AnyThrowable::of(error) {
        Some(exception) => exception_into_throw(exception, error),
        None => runtime_throw(bconcat!(context.as_ref(), ": ", error_chain_message(error))),
    }
}

/// `PluginInterface` adapter for a plugin entity living in the PHP child process: every
/// lifecycle call is forwarded as a `CallPhpMethod` RPC.
#[derive(Debug)]
pub struct PhpPluginProxy {
    pub(crate) phandle: u64,
    pub(crate) class: BString,
    /// Every interface the entity's class implements (`class_implements` in the child), for
    /// the `instanceof` checks PHP performs on plugin objects.
    pub(crate) implements: Vec<BString>,
}

impl PhpPluginProxy {
    pub fn new(phandle: u64, class: BString, implements: Vec<BString>) -> Self {
        Self {
            phandle,
            class,
            implements,
        }
    }

    fn forward_lifecycle_call(
        &self,
        method: &BStr,
        composer: &ComposerHandle,
        io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        let args = vec![composer_handle_value(composer), io_handle_value(io)?];
        let outcome = call_php_method(
            self.phandle,
            method,
            args,
            Some(&mut PluginRpcDispatcher::default()),
        )?;
        match outcome {
            Ok(_) => Ok(()),
            Err(throw) => Err(throw_into_error(throw)),
        }
    }

    /// For testing only: reads a public property of the plugin entity in the child, mirroring
    /// PHPUnit assertions like `$plugins[0]->version`.
    pub fn __get_property(&self, name: &BStr) -> anyhow::Result<PhpMixed> {
        let outcome = shirabe_php_rpc::call_function(
            "__shirabe_get_property",
            vec![
                PluginValue::PhpHandle(shirabe_php_rpc::PhpObjHandle {
                    phandle: self.phandle,
                    class: self.class.clone(),
                    implements: Vec::new(),
                }),
                PluginValue::string(name),
            ],
        )?;
        match outcome {
            Ok(value) => Ok(value.to_php_mixed()?),
            Err(throw) => Err(throw_into_error(throw)),
        }
    }
}

impl PluginInterface for PhpPluginProxy {
    fn activate(
        &mut self,
        composer: ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        self.forward_lifecycle_call(BStr::new("activate"), &composer, &io)
    }

    fn deactivate(
        &mut self,
        composer: ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        self.forward_lifecycle_call(BStr::new("deactivate"), &composer, &io)
    }

    fn uninstall(
        &mut self,
        composer: ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        self.forward_lifecycle_call(BStr::new("uninstall"), &composer, &io)
    }

    fn get_class_name(&self) -> BString {
        self.class.to_owned()
    }

    fn as_event_subscriber(&self) -> Option<&dyn EventSubscriberInterface> {
        if self
            .implements
            .iter()
            .any(|interface| interface == "Composer\\EventDispatcher\\EventSubscriberInterface")
        {
            Some(self)
        } else {
            None
        }
    }

    fn as_capable(&self) -> Option<&dyn Capable> {
        if self
            .implements
            .iter()
            .any(|interface| interface == "Composer\\Plugin\\Capable")
        {
            Some(self)
        } else {
            None
        }
    }

    fn __as_php_plugin_proxy(&self) -> Option<&PhpPluginProxy> {
        Some(self)
    }
}

impl EventSubscriberInterface for PhpPluginProxy {
    fn get_subscribed_events(&self) -> anyhow::Result<IndexMap<BString, SubscribedEventEntry>> {
        // PHP: `$subscriber->getSubscribedEvents()` (an instance call of the static method).
        let outcome = call_php_method(
            self.phandle,
            "getSubscribedEvents",
            Vec::new(),
            Some(&mut PluginRpcDispatcher::default()),
        )?;
        let value = match outcome {
            Ok(value) => value,
            Err(throw) => return Err(throw_into_error(throw)),
        };
        decode_subscribed_events(self.class.as_bstr(), value)
    }

    fn subscriber_handle(&self) -> PhpObjHandle {
        PhpObjHandle {
            phandle: self.phandle,
            class: self.class.clone(),
            implements: self.implements.clone(),
        }
    }
}

impl Capable for PhpPluginProxy {
    fn get_capabilities(&self) -> anyhow::Result<IndexMap<BString, PhpMixed>> {
        let outcome = call_php_method(
            self.phandle,
            "getCapabilities",
            Vec::new(),
            Some(&mut PluginRpcDispatcher::default()),
        )?;
        let value = match outcome {
            Ok(value) => value,
            Err(throw) => return Err(throw_into_error(throw)),
        };
        // PHP: `(array) $plugin->getCapabilities()` — the interface declares no return type,
        // so a non-array return is cast. A handle in the map (a plugin putting an object among
        // its capability values) has no PhpMixed image and fails the conversion explicitly.
        let entries = match value {
            PluginValue::Null => IndexMap::new(),
            PluginValue::Array(map) | PluginValue::Object(map) => map
                .into_iter()
                .map(|(k, v)| Ok((k, v.to_php_mixed()?)))
                .collect::<anyhow::Result<_>>()?,
            PluginValue::List(items) => items
                .into_iter()
                .enumerate()
                .map(|(i, v)| Ok((bconcat!(i), v.to_php_mixed()?)))
                .collect::<anyhow::Result<_>>()?,
            scalar => IndexMap::from([("0".into(), scalar.to_php_mixed()?)]),
        };
        Ok(entries)
    }
}

/// Decodes a `getSubscribedEvents()` wire value into the three shapes `addSubscriber`
/// distinguishes: `'method'`, `['method', priority]`, and `[['method', priority], ...]`.
/// A shape outside these is an explicit error, never a silently dropped listener.
fn decode_subscribed_events(
    class: &BStr,
    value: PluginValue,
) -> anyhow::Result<IndexMap<BString, SubscribedEventEntry>> {
    let entries = match value {
        PluginValue::Array(entries) => entries,
        PluginValue::List(items) => {
            // A PHP list-shaped array (integer keys) cannot map event names; an empty one is
            // the only legal case (an empty PHP array serializes as a list).
            if items.is_empty() {
                IndexMap::new()
            } else {
                return Err(subscribed_events_shape_error(
                    class,
                    &PluginValue::List(items),
                ));
            }
        }
        other => return Err(subscribed_events_shape_error(class, &other)),
    };
    let mut events: IndexMap<BString, SubscribedEventEntry> = IndexMap::new();
    for (event_name, params) in entries {
        let entry = match &params {
            PluginValue::String(method) => SubscribedEventEntry::Method(method.clone()),
            PluginValue::List(items) => match items.first() {
                Some(PluginValue::String(method)) => SubscribedEventEntry::MethodWithPriority(
                    method.clone(),
                    decode_listener_priority(class, items.get(1))?,
                ),
                _ => {
                    let mut listeners = Vec::with_capacity(items.len());
                    for listener in items {
                        let PluginValue::List(pair) = listener else {
                            return Err(subscribed_events_shape_error(class, &params));
                        };
                        let Some(PluginValue::String(method)) = pair.first() else {
                            return Err(subscribed_events_shape_error(class, &params));
                        };
                        listeners.push((
                            method.clone(),
                            decode_listener_priority(class, pair.get(1))?,
                        ));
                    }
                    SubscribedEventEntry::Methods(listeners)
                }
            },
            _ => return Err(subscribed_events_shape_error(class, &params)),
        };
        events.insert(event_name, entry);
    }
    Ok(events)
}

fn decode_listener_priority(
    class: &BStr,
    value: Option<&PluginValue>,
) -> anyhow::Result<Option<i64>> {
    match value {
        None => Ok(None),
        Some(PluginValue::Int(priority)) => Ok(Some(*priority)),
        Some(other) => Err(subscribed_events_shape_error(class, other)),
    }
}

fn subscribed_events_shape_error(class: &BStr, value: &PluginValue) -> anyhow::Error {
    RuntimeException::new(bconcat!(
        class,
        "::getSubscribedEvents() returned an unsupported shape over RPC: ",
        display_to_bytes(format_args!("{value:?}"))
    ))
    .into()
}

impl Drop for PhpPluginProxy {
    fn drop(&mut self) {
        // A dead worker has nothing left to release.
        let _ = release_php_handle(self.phandle);
    }
}

/// Wire value handing the shared Rust-side `$composer` to the child, interned in the R table.
pub fn composer_handle_value(composer: &ComposerHandle) -> PluginValue {
    rust_handle_value(register_composer_entity(composer), "Composer\\Composer")
}

/// Wire value handing the shared Rust-side `$io` to the child, interned in the R table.
pub fn io_handle_value(
    io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
) -> anyhow::Result<PluginValue> {
    let class = io_stub_class(io)?;
    Ok(rust_handle_value(register_io_entity(io), class))
}

/// Runs a boolean class query (`class_exists`, `is_subclass_of`, ...) in the worker with the
/// script autoloader active, so a class named by `composer.json` resolves through the Rust-side
/// [`ClassLoader`]s.
pub(crate) fn php_class_query(function: &BStr, args: Vec<PluginValue>) -> anyhow::Result<bool> {
    crate::event_dispatcher::EventDispatcher::ensure_script_autoloader()?;
    let value = unwrap_php_result(call_function_with_dispatcher(
        function,
        args,
        Some(&mut PluginRpcDispatcher::default()),
    ))?;
    match value {
        PluginValue::Bool(value) => Ok(value),
        other => Err(RuntimeException::new(bconcat!(
            "PHP class query `",
            function,
            "` did not return a bool: ",
            display_to_bytes(format_args!("{other:?}"))
        ))
        .into()),
    }
}

/// Instantiates `new $class(...$ctor_args)` in the worker, with the script autoloader active.
pub(crate) fn new_php_object(
    class: &BStr,
    ctor_args: Vec<PluginValue>,
) -> anyhow::Result<PhpObjHandle> {
    crate::event_dispatcher::EventDispatcher::ensure_script_autoloader()?;
    let value = unwrap_php_result(new_object(
        class,
        ctor_args,
        Some(&mut PluginRpcDispatcher::default()),
    ))?;
    match value {
        PluginValue::PhpHandle(handle) => Ok(handle),
        other => Err(RuntimeException::new(bconcat!(
            "`new ",
            class,
            "` returned an unsupported shape over RPC: ",
            display_to_bytes(format_args!("{other:?}"))
        ))
        .into()),
    }
}

/// Calls `$obj->$method(...$args)` on a worker-side entity.
pub(crate) fn call_php_entity_method(
    handle: &PhpObjHandle,
    method: &BStr,
    args: Vec<PluginValue>,
) -> anyhow::Result<PluginValue> {
    unwrap_php_result(call_php_method(
        handle.phandle,
        method,
        args,
        Some(&mut PluginRpcDispatcher::default()),
    ))
}

/// `is_a($obj, $class)` evaluated in the worker: the child's own class table answers, so
/// parent classes are covered (a `PhpObjHandle`'s `implements` lists interfaces only).
pub(crate) fn php_is_a(handle: &PhpObjHandle, class: &BStr) -> anyhow::Result<bool> {
    let value = unwrap_php_result(call_function_with_dispatcher(
        "is_a",
        vec![
            PluginValue::PhpHandle(handle.clone()),
            PluginValue::string(class),
        ],
        Some(&mut PluginRpcDispatcher::default()),
    ))?;
    Ok(matches!(value, PluginValue::Bool(true)))
}

/// Wire value handing a Rust-side repository to the child, interned in the R table.
pub(crate) fn repository_handle_value(
    repository: &RepositoryInterfaceHandle,
) -> Result<PluginValue, PhpThrow> {
    let class = repository_stub_class(repository)?;
    let rhandle = register_entity(RustEntity::Repository(repository.clone()));
    Ok(rust_handle_value(rhandle, class))
}

/// `InstallerInterface` adapter for an installer entity living in the PHP child process: the
/// installer a plugin hands to `InstallationManager::addInstaller`, or the class a legacy
/// `composer-installer` package names. Every call is forwarded as a `CallPhpMethod` RPC.
#[derive(Debug)]
pub struct PhpInstallerProxy {
    handle: PhpObjHandle,
}

impl PhpInstallerProxy {
    fn new(handle: PhpObjHandle) -> Self {
        Self { handle }
    }

    fn call(&self, method: &BStr, args: Vec<PluginValue>) -> anyhow::Result<PluginValue> {
        unwrap_php_result(call_php_method(
            self.handle.phandle,
            method,
            args,
            Some(&mut PluginRpcDispatcher::default()),
        ))
    }

    fn package_arg(package: &PackageInterfaceHandle) -> PluginValue {
        package_handle_value(package.as_rc())
    }

    fn optional_package_arg(package: &Option<PackageInterfaceHandle>) -> PluginValue {
        match package {
            Some(package) => Self::package_arg(package),
            None => PluginValue::Null,
        }
    }

    fn repo_arg(repo: &InstalledRepositoryInterfaceHandle) -> anyhow::Result<PluginValue> {
        Ok(repository_handle_value(&repo.as_repository_handle())?)
    }

    /// The `?PromiseInterface` half of the installer contract. The Rust callers await the
    /// installer's effects rather than chaining continuations, so a returned promise is awaited
    /// here as the PHP-owned job it crossed the boundary as.
    async fn promise_result(
        &self,
        method: &BStr,
        value: PluginValue,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let id = match value {
            PluginValue::Null => return Ok(None),
            PluginValue::Job(id) => id,
            other => return Err(self.unsupported_shape(method, &other)),
        };
        match job_registry::await_php_job(id).await? {
            PluginValue::Null => Ok(None),
            other => Ok(Some(other.to_php_mixed()?)),
        }
    }

    fn unsupported_shape(&self, method: &BStr, value: &PluginValue) -> anyhow::Error {
        RuntimeException::new(bconcat!(
            self.handle.class,
            "::",
            method,
            "() returned an unsupported shape over RPC: ",
            display_to_bytes(format_args!("{value:?}"))
        ))
        .into()
    }
}

#[async_trait::async_trait(?Send)]
impl crate::installer::InstallerInterface for PhpInstallerProxy {
    fn supports(&self, package_type: &bstr::BStr) -> anyhow::Result<bool> {
        match self.call(
            BStr::new("supports"),
            vec![PluginValue::string(package_type)],
        )? {
            PluginValue::Bool(supports) => Ok(supports),
            other => Err(self.unsupported_shape(BStr::new("supports"), &other)),
        }
    }

    fn is_installed(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<bool> {
        let args = vec![Self::repo_arg(repo)?, Self::package_arg(&package)];
        match self.call(BStr::new("isInstalled"), args)? {
            PluginValue::Bool(installed) => Ok(installed),
            other => Err(self.unsupported_shape(BStr::new("isInstalled"), &other)),
        }
    }

    async fn download(
        &self,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![
            Self::package_arg(&package),
            Self::optional_package_arg(&prev_package),
        ];
        let value = self.call(BStr::new("download"), args)?;
        self.promise_result(BStr::new("download"), value).await
    }

    async fn prepare(
        &self,
        r#type: &BStr,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![
            PluginValue::string(r#type),
            Self::package_arg(&package),
            Self::optional_package_arg(&prev_package),
        ];
        let value = self.call(BStr::new("prepare"), args)?;
        self.promise_result(BStr::new("prepare"), value).await
    }

    async fn install(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![Self::repo_arg(repo)?, Self::package_arg(&package)];
        let value = self.call(BStr::new("install"), args)?;
        self.promise_result(BStr::new("install"), value).await
    }

    async fn update(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![
            Self::repo_arg(repo)?,
            Self::package_arg(&initial),
            Self::package_arg(&target),
        ];
        let value = self.call(BStr::new("update"), args)?;
        self.promise_result(BStr::new("update"), value).await
    }

    async fn uninstall(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![Self::repo_arg(repo)?, Self::package_arg(&package)];
        let value = self.call(BStr::new("uninstall"), args)?;
        self.promise_result(BStr::new("uninstall"), value).await
    }

    async fn cleanup(
        &self,
        r#type: &BStr,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![
            PluginValue::string(r#type),
            Self::package_arg(&package),
            Self::optional_package_arg(&prev_package),
        ];
        let value = self.call(BStr::new("cleanup"), args)?;
        self.promise_result(BStr::new("cleanup"), value).await
    }

    fn get_install_path(&self, package: PackageInterfaceHandle) -> Option<std::path::PathBuf> {
        // PHP declares `getInstallPath(): string`; a failure here is a plugin error the
        // infallible signature cannot carry, so it aborts rather than answering a path that
        // would silently install the package in the wrong place.
        let args = vec![Self::package_arg(&package)];
        let value = self
            .call(BStr::new("getInstallPath"), args)
            .unwrap_or_else(|error| {
                panic!(
                    "{}::getInstallPath failed over RPC: {error:#}",
                    self.handle.class.display_lossy()
                )
            });
        match value {
            PluginValue::Null => None,
            PluginValue::String(path) => Some(std::path::PathBuf::from(path_from_bytes(&path))),
            other => panic!(
                "{}",
                self.unsupported_shape(BStr::new("getInstallPath"), &other)
            ),
        }
    }
}

impl Drop for PhpInstallerProxy {
    fn drop(&mut self) {
        let _ = release_php_handle(self.handle.phandle);
    }
}

thread_local! {
    /// The installer adapters handed to `InstallationManager::addInstaller` over RPC, keyed by
    /// the entity's phandle. `removeInstaller` arrives carrying the same entity, and the
    /// manager compares installers by identity, so the adapter it was given has to be found
    /// again rather than rebuilt.
    static PHP_INSTALLER_PROXIES: std::cell::RefCell<IndexMap<u64, std::rc::Rc<dyn crate::installer::InstallerInterface>>> =
        std::cell::RefCell::new(IndexMap::new());
}

/// The adapter for an installer entity, building it on first sight.
pub(crate) fn php_installer_proxy(
    handle: PhpObjHandle,
) -> std::rc::Rc<dyn crate::installer::InstallerInterface> {
    PHP_INSTALLER_PROXIES.with(|proxies| {
        let mut proxies = proxies.borrow_mut();
        if let Some(existing) = proxies.get(&handle.phandle) {
            return existing.clone();
        }
        let phandle = handle.phandle;
        let proxy: std::rc::Rc<dyn crate::installer::InstallerInterface> =
            std::rc::Rc::new(PhpInstallerProxy::new(handle));
        proxies.insert(phandle, proxy.clone());
        proxy
    })
}

/// Drops the adapter bookkeeping for an installer entity that left the manager.
fn forget_php_installer_proxy(phandle: u64) {
    PHP_INSTALLER_PROXIES.with(|proxies| {
        proxies.borrow_mut().shift_remove(&phandle);
    });
}

/// `Capability` adapter for a capability entity living in the PHP child process, for
/// capability interfaces that add no methods of their own (the plain
/// `Composer\Plugin\Capability\Capability` marker).
#[derive(Debug)]
pub struct PhpCapabilityProxy {
    handle: PhpObjHandle,
}

impl PhpCapabilityProxy {
    pub(crate) fn new(handle: PhpObjHandle) -> Self {
        Self { handle }
    }

    /// For testing only: the entity descriptor, whose class and interface list answer the
    /// `assertInstanceOf` checks PHPUnit makes on a capability.
    pub fn __handle(&self) -> &PhpObjHandle {
        &self.handle
    }

    /// For testing only: reads a public property of the capability entity in the child. Unlike
    /// `PhpPluginProxy::__get_property` the value keeps its wire form, so a test can assert the
    /// object identity behind a handle instead of only the plain data around it.
    pub fn __get_property(&self, name: &BStr) -> anyhow::Result<PluginValue> {
        let outcome = shirabe_php_rpc::call_function(
            "__shirabe_get_property",
            vec![
                PluginValue::PhpHandle(self.handle.clone()),
                PluginValue::string(name),
            ],
        )?;
        match outcome {
            Ok(value) => Ok(value),
            Err(throw) => Err(throw_into_error(throw)),
        }
    }
}

impl Capability for PhpCapabilityProxy {
    fn __as_php_capability_proxy(&self) -> Option<&PhpCapabilityProxy> {
        Some(self)
    }
}

impl Drop for PhpCapabilityProxy {
    fn drop(&mut self) {
        let _ = release_php_handle(self.handle.phandle);
    }
}

/// `CommandProvider` adapter for a capability entity living in the PHP child process.
#[derive(Debug)]
pub struct PhpCommandProviderProxy {
    handle: PhpObjHandle,
}

impl PhpCommandProviderProxy {
    pub(crate) fn new(handle: PhpObjHandle) -> Self {
        Self { handle }
    }
}

impl Capability for PhpCommandProviderProxy {
    fn as_command_provider(&self) -> Option<&dyn CommandProvider> {
        Some(self)
    }
}

impl CommandProvider for PhpCommandProviderProxy {
    fn get_commands(
        &self,
    ) -> anyhow::Result<Vec<std::rc::Rc<std::cell::RefCell<dyn BaseCommand>>>> {
        let value = unwrap_php_result(call_php_method(
            self.handle.phandle,
            "getCommands",
            Vec::new(),
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        // PHP's getCommands(): array is unenforceable on the wire, and a
        // `Vec<Box<dyn BaseCommand>>` asserts every element up front, so the two checks
        // Application::getPluginCommands performs on the raw value live here, with its
        // messages.
        let items = match value {
            PluginValue::List(items) => items,
            PluginValue::Array(map) => map.into_values().collect(),
            _ => {
                return Err(UnexpectedValueException::new(bconcat!(
                    "Plugin capability ",
                    self.handle.class,
                    " failed to return an array from getCommands"
                ))
                .into());
            }
        };
        let mut commands: Vec<std::rc::Rc<std::cell::RefCell<dyn BaseCommand>>> = Vec::new();
        for item in items {
            let command_handle = match item {
                PluginValue::PhpHandle(handle) => handle,
                _ => return Err(invalid_command_error(&self.handle)),
            };
            if !php_is_a(&command_handle, BStr::new("Composer\\Command\\BaseCommand"))? {
                return Err(invalid_command_error(&self.handle));
            }
            commands.push(std::rc::Rc::new(std::cell::RefCell::new(
                PhpCommandProxy::new(command_handle)?,
            )));
        }
        Ok(commands)
    }
}

fn invalid_command_error(capability: &PhpObjHandle) -> anyhow::Error {
    UnexpectedValueException::new(bconcat!(
        "Plugin capability ",
        capability.class,
        " returned an invalid value, we expected an array of Composer\\Command\\BaseCommand objects"
    ))
    .into()
}

impl Drop for PhpCommandProviderProxy {
    fn drop(&mut self) {
        let _ = release_php_handle(self.handle.phandle);
    }
}

/// Metadata row for one Rust-implemented command, mirrored into the worker as a
/// `\Shirabe\RustCommandStub` so a worker-hosted command can `find()` and invoke built-in
/// commands (their execution crosses back into this process).
#[derive(Debug)]
pub(crate) struct RustCommandMetadata {
    pub(crate) name: BString,
    pub(crate) description: BString,
    pub(crate) aliases: Vec<BString>,
    pub(crate) hidden: bool,
}

impl RustCommandMetadata {
    fn wire_value(&self) -> PluginValue {
        let mut row: IndexMap<BString, PluginValue> = IndexMap::new();
        row.insert("name".into(), PluginValue::string(self.name.clone()));
        row.insert(
            "description".into(),
            PluginValue::string(self.description.clone()),
        );
        row.insert(
            "aliases".into(),
            PluginValue::List(
                self.aliases
                    .iter()
                    .map(|alias| PluginValue::string(alias.clone()))
                    .collect(),
            ),
        );
        row.insert("hidden".into(), PluginValue::Bool(self.hidden));
        PluginValue::Array(row)
    }
}

/// Handoff state for the worker-side console application (the `Composer\Console\Application`
/// defined under the RPC crate's `php/runtime/`): assembled by
/// `Application::register_worker_console_commands` as worker-hosted commands are registered,
/// booted in the worker the first time one of them actually runs.
#[derive(Debug)]
struct PhpConsoleApplicationContext {
    composer: Option<ComposerHandle>,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    initial_working_directory: Option<std::path::PathBuf>,
    disable_plugins_by_default: bool,
    disable_scripts_by_default: bool,
    rust_commands: Vec<RustCommandMetadata>,
    /// Clones of the worker-hosted command handles; ownership (and release) stays with the
    /// `PhpCommandProxy` instances holding the originals.
    plugin_commands: std::cell::RefCell<Vec<PhpObjHandle>>,
    app: std::cell::RefCell<Option<PhpObjHandle>>,
}

thread_local! {
    /// Handles of the `PhpCommandProxy` instances built since the last drain; drained into the
    /// console application context by `Application::register_worker_console_commands`.
    static PENDING_COMMAND_HANDLES: std::cell::RefCell<Vec<PhpObjHandle>> =
        const { std::cell::RefCell::new(Vec::new()) };

    /// The published context, read by `PhpCommandProxy::run` at execution time.
    static CONSOLE_APP_CONTEXT: std::cell::RefCell<Option<std::rc::Rc<PhpConsoleApplicationContext>>> =
        const { std::cell::RefCell::new(None) };
}

/// Clears handles a failed earlier collection may have left behind.
pub(crate) fn reset_pending_plugin_command_handles() {
    PENDING_COMMAND_HANDLES.with(|handles| handles.borrow_mut().clear());
}

pub(crate) fn take_pending_plugin_command_handles() -> Vec<PhpObjHandle> {
    PENDING_COMMAND_HANDLES.with(|handles| std::mem::take(&mut *handles.borrow_mut()))
}

pub(crate) fn publish_console_application_context(
    composer: Option<&ComposerHandle>,
    io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    initial_working_directory: Option<std::path::PathBuf>,
    disable_plugins_by_default: bool,
    disable_scripts_by_default: bool,
    rust_commands: Vec<RustCommandMetadata>,
    plugin_commands: Vec<PhpObjHandle>,
) {
    let context = std::rc::Rc::new(PhpConsoleApplicationContext {
        composer: composer.cloned(),
        io: io.clone(),
        initial_working_directory,
        disable_plugins_by_default,
        disable_scripts_by_default,
        rust_commands,
        plugin_commands: std::cell::RefCell::new(plugin_commands),
        app: std::cell::RefCell::new(None),
    });
    CONSOLE_APP_CONTEXT.with(|slot| *slot.borrow_mut() = Some(context));
}

/// Adds command entities to the published handoff, for commands registered after it was
/// published. A worker-side application that already booted gets them right away, so the two
/// sides keep the same command set.
pub(crate) fn extend_console_application_commands(
    handles: Vec<PhpObjHandle>,
) -> anyhow::Result<()> {
    let context = CONSOLE_APP_CONTEXT
        .with(|slot| slot.borrow().clone())
        .expect("only an application that published the handoff extends it");
    for handle in handles {
        if let Some(app) = context.app.borrow().as_ref() {
            call_php_entity_method(
                app,
                BStr::new("add"),
                vec![PluginValue::PhpHandle(handle.clone())],
            )?;
        }
        context.plugin_commands.borrow_mut().push(handle);
    }
    Ok(())
}

impl PhpConsoleApplicationContext {
    /// Boots the worker-side application on first use and returns its handle.
    fn booted_app(&self) -> anyhow::Result<PhpObjHandle> {
        if let Some(app) = self.app.borrow().as_ref() {
            return Ok(app.clone());
        }
        let mut config: IndexMap<BString, PluginValue> = IndexMap::new();
        config.insert(
            "composer".into(),
            match &self.composer {
                Some(composer) => composer_handle_value(composer),
                None => PluginValue::Null,
            },
        );
        config.insert("io".into(), io_handle_value(&self.io)?);
        config.insert(
            "initialWorkingDirectory".into(),
            match &self.initial_working_directory {
                Some(dir) => PluginValue::String(dir.as_os_str().as_encoded_bytes().into()),
                None => PluginValue::Null,
            },
        );
        config.insert(
            "disablePluginsByDefault".into(),
            PluginValue::Bool(self.disable_plugins_by_default),
        );
        config.insert(
            "disableScriptsByDefault".into(),
            PluginValue::Bool(self.disable_scripts_by_default),
        );
        config.insert(
            "rustCommands".into(),
            PluginValue::List(
                self.rust_commands
                    .iter()
                    .map(RustCommandMetadata::wire_value)
                    .collect(),
            ),
        );
        config.insert(
            "pluginCommands".into(),
            PluginValue::List(
                self.plugin_commands
                    .borrow()
                    .iter()
                    .cloned()
                    .map(PluginValue::PhpHandle)
                    .collect(),
            ),
        );
        let value = unwrap_php_result(call_function_with_dispatcher(
            "__shirabe_console_application_boot",
            vec![PluginValue::Array(config)],
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        let app = match value {
            PluginValue::PhpHandle(app) => app,
            other => {
                return Err(RuntimeException::new(bconcat!(
                    "__shirabe_console_application_boot returned an unsupported shape over RPC: ",
                    display_to_bytes(format_args!("{other:?}"))
                ))
                .into());
            }
        };
        *self.app.borrow_mut() = Some(app.clone());
        Ok(app)
    }
}

impl Drop for PhpConsoleApplicationContext {
    fn drop(&mut self) {
        if let Some(app) = self.app.borrow_mut().take() {
            let _ = release_php_handle(app.phandle);
        }
    }
}

/// `BaseCommand` adapter for a command entity living in the PHP child process. The Rust-side
/// command state mirrors the child's metadata (name, description, aliases, hidden/proxy flags,
/// help, usages and the input definition — read back over RPC at construction, after the PHP
/// constructor ran `configure()`), so `list` and `help` render from local state; running the
/// command forwards the whole input line to the worker-side console application.
#[derive(Debug)]
pub struct PhpCommandProxy {
    base_command_data: crate::command::BaseCommandData,
    handle: PhpObjHandle,
    proxy_command: bool,
}

impl PhpCommandProxy {
    fn new(handle: PhpObjHandle) -> anyhow::Result<Self> {
        let proxy_command = match Self::call_metadata_getter(&handle, BStr::new("isProxyCommand"))?
        {
            PluginValue::Bool(proxy_command) => proxy_command,
            other => {
                return Err(Self::unsupported_shape(
                    &handle,
                    BStr::new("isProxyCommand"),
                    &other,
                ));
            }
        };
        Self::build(handle, proxy_command)
    }

    /// A command class named by a `composer.json` script only has to extend Symfony's `Command`,
    /// so `isProxyCommand()` is asked for only when it also extends Composer's `BaseCommand`.
    pub(crate) fn new_script_command(handle: PhpObjHandle) -> anyhow::Result<Self> {
        let proxy_command = if php_is_a(&handle, BStr::new("Composer\\Command\\BaseCommand"))? {
            match Self::call_metadata_getter(&handle, BStr::new("isProxyCommand"))? {
                PluginValue::Bool(proxy_command) => proxy_command,
                other => {
                    return Err(Self::unsupported_shape(
                        &handle,
                        BStr::new("isProxyCommand"),
                        &other,
                    ));
                }
            }
        } else {
            false
        };
        Self::build(handle, proxy_command)
    }

    fn build(handle: PhpObjHandle, proxy_command: bool) -> anyhow::Result<Self> {
        let data = crate::command::BaseCommandData::new(None);
        let name = Self::call_metadata_getter(&handle, BStr::new("getName"))?;
        match name {
            PluginValue::Null => {}
            PluginValue::String(bytes) => {
                Command::set_name(&data, bytes.as_bstr())?;
            }
            other => {
                return Err(Self::unsupported_shape(
                    &handle,
                    BStr::new("getName"),
                    &other,
                ));
            }
        }
        let description = Self::call_metadata_getter(&handle, BStr::new("getDescription"))?;
        match description {
            PluginValue::String(bytes) => {
                Command::set_description(&data, bytes.as_bstr());
            }
            other => {
                return Err(Self::unsupported_shape(
                    &handle,
                    BStr::new("getDescription"),
                    &other,
                ));
            }
        }
        let aliases = Self::call_metadata_getter(&handle, BStr::new("getAliases"))?;
        let alias_items = match aliases {
            PluginValue::List(items) => items,
            PluginValue::Array(map) => map.into_values().collect(),
            other => {
                return Err(Self::unsupported_shape(
                    &handle,
                    BStr::new("getAliases"),
                    &other,
                ));
            }
        };
        let mut alias_names = Vec::new();
        for alias in alias_items {
            match alias {
                PluginValue::String(bytes) => {
                    alias_names.push(bytes);
                }
                other => {
                    return Err(Self::unsupported_shape(
                        &handle,
                        BStr::new("getAliases"),
                        &other,
                    ));
                }
            }
        }
        Command::set_aliases(&data, alias_names)?;
        let hidden = Self::call_metadata_getter(&handle, BStr::new("isHidden"))?;
        match hidden {
            PluginValue::Bool(hidden) => {
                Command::set_hidden(&data, hidden);
            }
            other => {
                return Err(Self::unsupported_shape(
                    &handle,
                    BStr::new("isHidden"),
                    &other,
                ));
            }
        }
        Self::read_back_definition(&handle, &data)?;
        PENDING_COMMAND_HANDLES.with(|handles| handles.borrow_mut().push(handle.clone()));
        Ok(Self {
            base_command_data: data,
            handle,
            proxy_command,
        })
    }

    /// Mirrors the command's input definition (plus help text and extra usages) into the
    /// Rust-side command state, so `help`/`list` render it without touching the worker.
    fn read_back_definition(
        handle: &PhpObjHandle,
        data: &crate::command::BaseCommandData,
    ) -> anyhow::Result<()> {
        use shirabe_symfony_console::input::{
            DefinitionItem, InputArgument, InputDefinition, InputOption,
        };

        let value = unwrap_php_result(call_function_with_dispatcher(
            "__shirabe_read_command_definition",
            vec![PluginValue::PhpHandle(handle.clone())],
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        let mut map = match value {
            PluginValue::Array(map) => map,
            other => {
                return Err(Self::unsupported_shape(
                    handle,
                    BStr::new("getDefinition"),
                    &other,
                ));
            }
        };
        let field = |row: &mut IndexMap<BString, PluginValue>, key: &BStr| -> PluginValue {
            row.shift_remove(key.as_bytes())
                .unwrap_or(PluginValue::Null)
        };
        let as_rows = |value: PluginValue| -> Vec<PluginValue> {
            match value {
                PluginValue::List(rows) => rows,
                PluginValue::Array(map) => map.into_values().collect(),
                _ => Vec::new(),
            }
        };

        let mut items: Vec<DefinitionItem> = Vec::new();
        for row in as_rows(field(&mut map, BStr::new("arguments"))) {
            let mut row = match row {
                PluginValue::Array(row) => row,
                other => {
                    return Err(Self::unsupported_shape(
                        handle,
                        BStr::new("getDefinition"),
                        &other,
                    ));
                }
            };
            let (name, description) = match (
                field(&mut row, BStr::new("name")),
                field(&mut row, BStr::new("description")),
            ) {
                (PluginValue::String(name), PluginValue::String(description)) => {
                    (name, description)
                }
                (other, _) => {
                    return Err(Self::unsupported_shape(
                        handle,
                        BStr::new("getDefinition"),
                        &other,
                    ));
                }
            };
            let required = matches!(
                field(&mut row, BStr::new("required")),
                PluginValue::Bool(true)
            );
            let is_array = matches!(
                field(&mut row, BStr::new("isArray")),
                PluginValue::Bool(true)
            );
            let mut mode = if required {
                InputArgument::REQUIRED
            } else {
                InputArgument::OPTIONAL
            };
            if is_array {
                mode |= InputArgument::IS_ARRAY;
            }
            let default =
                InputValue::from_php_mixed(&field(&mut row, BStr::new("default")).to_php_mixed()?);
            items.push(DefinitionItem::InputArgument(InputArgument::new(
                name,
                Some(mode),
                description,
                default,
            )?));
        }
        for row in as_rows(field(&mut map, BStr::new("options"))) {
            let mut row = match row {
                PluginValue::Array(row) => row,
                other => {
                    return Err(Self::unsupported_shape(
                        handle,
                        BStr::new("getDefinition"),
                        &other,
                    ));
                }
            };
            let (name, description) = match (
                field(&mut row, BStr::new("name")),
                field(&mut row, BStr::new("description")),
            ) {
                (PluginValue::String(name), PluginValue::String(description)) => {
                    (name, description)
                }
                (other, _) => {
                    return Err(Self::unsupported_shape(
                        handle,
                        BStr::new("getDefinition"),
                        &other,
                    ));
                }
            };
            let accept_value = matches!(
                field(&mut row, BStr::new("acceptValue")),
                PluginValue::Bool(true)
            );
            let mut mode = if accept_value {
                if matches!(
                    field(&mut row, BStr::new("isValueRequired")),
                    PluginValue::Bool(true)
                ) {
                    InputOption::VALUE_REQUIRED
                } else {
                    InputOption::VALUE_OPTIONAL
                }
            } else {
                InputOption::VALUE_NONE
            };
            if matches!(
                field(&mut row, BStr::new("isArray")),
                PluginValue::Bool(true)
            ) {
                mode |= InputOption::VALUE_IS_ARRAY;
            }
            if matches!(
                field(&mut row, BStr::new("isNegatable")),
                PluginValue::Bool(true)
            ) {
                mode |= InputOption::VALUE_NEGATABLE;
            }
            let shortcut = match field(&mut row, BStr::new("shortcut")).to_php_mixed()? {
                PhpMixed::String(shortcut) => Some(shortcut),
                _ => None,
            };
            // `getDefault()` exposes the stored representation (`false` for VALUE_NONE), while
            // the constructor only accepts null there; mirror the constructor's normalization.
            let default = if accept_value {
                InputValue::from_php_mixed(&field(&mut row, BStr::new("default")).to_php_mixed()?)
            } else {
                InputValue::Null
            };
            items.push(DefinitionItem::InputOption(InputOption::new(
                &name,
                shortcut.as_ref().map(|shortcut| shortcut.as_bstr()),
                Some(mode),
                description,
                default,
            )?));
        }
        data.command_data().set_definition(
            shirabe_symfony_console::command::SetDefinitionArg::Definition(InputDefinition::new(
                items,
            )?),
        );

        match field(&mut map, BStr::new("help")) {
            PluginValue::String(help) => {
                Command::set_help(data, help.as_bstr());
            }
            other => {
                return Err(Self::unsupported_shape(
                    handle,
                    BStr::new("getHelp"),
                    &other,
                ));
            }
        }
        for usage in as_rows(field(&mut map, BStr::new("usages"))) {
            match usage {
                PluginValue::String(usage) => {
                    Command::add_usage(data, usage.as_bstr());
                }
                other => {
                    return Err(Self::unsupported_shape(
                        handle,
                        BStr::new("getUsages"),
                        &other,
                    ));
                }
            }
        }
        Ok(())
    }

    fn call_metadata_getter(handle: &PhpObjHandle, method: &BStr) -> anyhow::Result<PluginValue> {
        unwrap_php_result(call_php_method(
            handle.phandle,
            method,
            Vec::new(),
            Some(&mut PluginRpcDispatcher::default()),
        ))
    }

    fn unsupported_shape(
        handle: &PhpObjHandle,
        method: &BStr,
        value: &PluginValue,
    ) -> anyhow::Error {
        RuntimeException::new(bconcat!(
            handle.class,
            "::",
            method,
            "() returned an unsupported shape over RPC: ",
            display_to_bytes(format_args!("{value:?}"))
        ))
        .into()
    }
}

#[async_trait::async_trait(?Send)]
impl Command for PhpCommandProxy {
    /// Forwards the whole run to the worker-side console application (the proxy-command idiom
    /// the trait allows): the real Symfony machinery there performs input binding, validation,
    /// interaction and execution against the live PHP command object, writing to the stdio the
    /// worker inherited. The Rust-side `base_run` half must not run against the mirrored
    /// definition, or binding and interaction would happen twice.
    async fn run(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let context = CONSOLE_APP_CONTEXT
            .with(|slot| slot.borrow().clone())
            .ok_or_else(|| -> anyhow::Error {
                RuntimeException::new(bconcat!(
                    "cannot run plugin-provided command ",
                    self.handle.class,
                    ": no worker-side console application context was published"
                ))
                .into()
            })?;
        let app = context.booted_app()?;
        let input_line = input.borrow().__to_string();
        let value = unwrap_php_result(call_function_with_dispatcher(
            "__shirabe_run_console_application",
            vec![PluginValue::PhpHandle(app), PluginValue::string(input_line)],
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        match value {
            PluginValue::Int(code) => Ok(code),
            other => Err(Self::unsupported_shape(
                &self.handle,
                BStr::new("run"),
                &other,
            )),
        }
    }

    async fn execute(
        &self,
        _input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        // `run` above never reaches this template hook; a direct call would bypass the
        // worker-side binding, so it stays an explicit error.
        Err(RuntimeException::new(bconcat!(
            "plugin-provided command ",
            self.handle.class,
            " executes in the PHP worker through run(); execute() must not be called directly"
        ))
        .into())
    }

    fn is_proxy_command(&self) -> bool {
        self.proxy_command
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for PhpCommandProxy {
    fn base_command_data(&self) -> &crate::command::BaseCommandData {
        &self.base_command_data
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}

impl PhpClass for PhpCommandProxy {
    fn php_class_name(&self) -> &BStr {
        BStr::new(&self.handle.class)
    }
}

impl Drop for PhpCommandProxy {
    fn drop(&mut self) {
        let _ = release_php_handle(self.handle.phandle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_data_argument_keeps_its_bytes() {
        let bytes = BString::from(b"vendor/\xffname".as_slice());
        let args = [PluginValue::String(bytes.clone())];

        assert_eq!(
            bytes,
            arg::<BString>(BStr::new("method"), &args, 0).unwrap()
        );
    }

    #[test]
    fn a_process_command_keeps_its_bytes() {
        let bytes = BString::from(b"echo \xff".as_slice());

        match exec_command_arg(
            BStr::new("execute"),
            &[PluginValue::String(bytes.clone())],
            0,
        )
        .unwrap()
        {
            crate::util::process_executor::CommandLine::Shell(command) => {
                assert_eq!(bytes, command)
            }
            other => panic!("expected a shell command, got {other:?}"),
        }
    }

    #[test]
    fn a_listener_method_name_that_is_not_utf8_keeps_its_bytes() {
        let method = BString::from(b"on\xffEvent".as_slice());
        let events = decode_subscribed_events(
            BStr::new("Plugin"),
            PluginValue::Array(IndexMap::from([
                (BString::from("a"), PluginValue::String(method.clone())),
                (
                    BString::from("b"),
                    PluginValue::List(vec![
                        PluginValue::String(method.clone()),
                        PluginValue::Int(1),
                    ]),
                ),
                (
                    BString::from("c"),
                    PluginValue::List(vec![PluginValue::List(vec![PluginValue::String(
                        method.clone(),
                    )])]),
                ),
            ])),
        )
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
        let events = decode_subscribed_events(
            BStr::new("Vendor\\Plugin"),
            PluginValue::Array(IndexMap::from([(
                name.clone(),
                PluginValue::String(BString::from("method")),
            )])),
        )
        .unwrap();
        assert!(events.contains_key(&name));
    }

    #[test]
    fn a_method_name_that_is_not_utf8_reaches_the_error_unchanged() {
        let name = b"method\xff";
        let throw = PluginRpcDispatcher::default()
            .dispatch(0, BStr::new(name), Vec::new(), &[], &mut IndexMap::new())
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
