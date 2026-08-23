//! PHP-backed `PluginInterface` adapter and the R table serving its RPC callbacks.
//!
//! This module has no Composer counterpart: it is the Rust half of the plugin runtime split
//! (a real PHP child process executes the plugin code, see `docs/dev/php-rpc.md`). The plugin
//! entity lives in the worker's P table; Rust-side entities the plugin can call back into
//! (`$composer`, `$io`) live in the R table here.

use crate::autoload::ClassLoader;
use crate::command::BaseCommand;
use crate::composer::ComposerHandle;
use crate::dependency_resolver::operation::AnyOperation;
use crate::event_dispatcher::event_dispatcher::dispatch_event_method;
use crate::event_dispatcher::{
    EventInterface, EventSubscriberInterface, SubscribedEventEntry, unwrap_php_result,
};
use crate::installer::InstallationManagerInterface;
use crate::io::IOInterface;
use crate::package::handle::AnyPackage;
use crate::package::{DisplayMode, PackageInterfaceHandle};
use crate::plugin::capability::{Capability, CommandProvider};
use crate::plugin::capable::Capable;
use crate::plugin::php_plugin_value::{
    date_time_from_wire, date_time_to_wire, link_from_wire, link_to_wire,
};
use crate::plugin::plugin_interface::PluginInterface;
use crate::repository::{
    InstalledArrayRepository, InstalledFilesystemRepository, InstalledRepositoryInterfaceHandle,
    RepositoryInterfaceHandle, RepositoryManagerInterface,
};
use indexmap::IndexMap;
use shirabe_php_rpc::{
    PhpObjHandle, PhpThrow, PluginValue, RustMethodDispatcher, RustObjHandle,
    call_function_with_dispatcher, call_php_method, new_object, release_php_handle,
};
use shirabe_php_shim::PhpMixed;
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::InputInterface;
use shirabe_symfony_console::input::InputValue;
use shirabe_symfony_console::output::OutputInterface;

/// A Rust-side entity a PHP proxy stub points back to.
#[derive(Debug, Clone)]
enum RustEntity {
    Composer(ComposerHandle),
    Config(std::rc::Rc<std::cell::RefCell<crate::config::Config>>),
    DownloadManager(
        std::rc::Rc<std::cell::RefCell<dyn crate::downloader::DownloadManagerInterface>>,
    ),
    Filesystem(std::rc::Rc<std::cell::RefCell<crate::util::Filesystem>>),
    Io(std::rc::Rc<std::cell::RefCell<dyn IOInterface>>),
    InstallationManager(std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>>),
    RepositoryManager(std::rc::Rc<std::cell::RefCell<dyn RepositoryManagerInterface>>),
    Repository(RepositoryInterfaceHandle),
    Package(std::rc::Rc<std::cell::RefCell<AnyPackage>>),
    EventDispatcher(
        std::rc::Rc<std::cell::RefCell<dyn crate::event_dispatcher::EventDispatcherInterface>>,
    ),
    Operation(std::rc::Rc<AnyOperation>),
    Plugin(std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>),
}

/// The pointer identity backing R-table interning: the same shared instance must always cross
/// the boundary as the same handle (`===` in the child).
fn entity_ptr_id(entity: &RustEntity) -> usize {
    match entity {
        RustEntity::Composer(composer) => {
            std::rc::Rc::as_ptr(composer.as_rc()) as *const () as usize
        }
        RustEntity::Config(config) => std::rc::Rc::as_ptr(config) as *const () as usize,
        RustEntity::DownloadManager(dm) => std::rc::Rc::as_ptr(dm) as *const () as usize,
        RustEntity::Filesystem(fs) => std::rc::Rc::as_ptr(fs) as *const () as usize,
        RustEntity::Io(io) => std::rc::Rc::as_ptr(io) as *const () as usize,
        RustEntity::InstallationManager(im) => std::rc::Rc::as_ptr(im) as *const () as usize,
        RustEntity::RepositoryManager(rm) => std::rc::Rc::as_ptr(rm) as *const () as usize,
        RustEntity::Repository(repository) => repository.ptr_id(),
        RustEntity::Package(package) => std::rc::Rc::as_ptr(package) as *const () as usize,
        RustEntity::EventDispatcher(dispatcher) => {
            std::rc::Rc::as_ptr(dispatcher) as *const () as usize
        }
        RustEntity::Operation(operation) => std::rc::Rc::as_ptr(operation) as *const () as usize,
        RustEntity::Plugin(plugin) => std::rc::Rc::as_ptr(plugin) as *const () as usize,
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

/// A `RustHandle` wire descriptor for a freshly registered (or re-interned) entity.
pub(crate) fn rust_handle_value(rhandle: u64, class: &str) -> PluginValue {
    PluginValue::RustHandle(RustObjHandle {
        rhandle,
        class: class.to_string(),
        epoch: 0,
        snapshot: None,
    })
}

/// The proxy stub class matching a package's concrete variant.
fn package_stub_class(package: &std::rc::Rc<std::cell::RefCell<AnyPackage>>) -> &'static str {
    match &*package.borrow() {
        AnyPackage::Package(_) => "Composer\\Package\\Package",
        AnyPackage::CompletePackage(_) => "Composer\\Package\\CompletePackage",
        AnyPackage::RootPackage(_) => "Composer\\Package\\RootPackage",
        AnyPackage::AliasPackage(_) => "Composer\\Package\\AliasPackage",
        AnyPackage::CompleteAliasPackage(_) => "Composer\\Package\\CompleteAliasPackage",
        AnyPackage::RootAliasPackage(_) => "Composer\\Package\\RootAliasPackage",
    }
}

/// The proxy stub class matching a repository's concrete type.
fn repository_stub_class(repository: &RepositoryInterfaceHandle) -> Result<&'static str, PhpThrow> {
    if repository.is::<InstalledFilesystemRepository>() {
        Ok("Composer\\Repository\\InstalledFilesystemRepository")
    } else if repository.is::<InstalledArrayRepository>() {
        Ok("Composer\\Repository\\InstalledArrayRepository")
    } else {
        // TODO(plugin): the remaining repository classes get stubs on demand, driven by
        // explicit errors from real plugins.
        Err(runtime_throw(
            "no proxy stub class is available for this repository over RPC yet".to_string(),
        ))
    }
}

/// The proxy stub class matching a solver operation's concrete type.
fn operation_stub_class(operation: &AnyOperation) -> &'static str {
    match operation {
        AnyOperation::Install(_) => "Composer\\DependencyResolver\\Operation\\InstallOperation",
        AnyOperation::Update(_) => "Composer\\DependencyResolver\\Operation\\UpdateOperation",
        AnyOperation::Uninstall(_) => "Composer\\DependencyResolver\\Operation\\UninstallOperation",
        AnyOperation::MarkAliasInstalled(_) => {
            "Composer\\DependencyResolver\\Operation\\MarkAliasInstalledOperation"
        }
        AnyOperation::MarkAliasUninstalled(_) => {
            "Composer\\DependencyResolver\\Operation\\MarkAliasUninstalledOperation"
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
        Some(_) => "Shirabe\\RustCapablePluginStub",
        None => "Shirabe\\RustPluginStub",
    };
    let rhandle = register_entity(RustEntity::Plugin(plugin.clone()));
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
) -> anyhow::Result<&'static str> {
    let borrowed = io.borrow();
    let any = borrowed.as_any();
    if any
        .downcast_ref::<crate::io::buffer_io::BufferIO>()
        .is_some()
    {
        Ok("Composer\\IO\\BufferIO")
    } else if any
        .downcast_ref::<crate::io::console_io::ConsoleIO>()
        .is_some()
    {
        Ok("Composer\\IO\\ConsoleIO")
    } else if any.downcast_ref::<crate::io::null_io::NullIO>().is_some() {
        Ok("Composer\\IO\\NullIO")
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
pub(crate) fn find_file_in_registered_loaders(class: &str) -> Option<String> {
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
    pub(crate) event: Option<(u64, &'a dyn EventInterface)>,
}

impl RustMethodDispatcher for PluginRpcDispatcher<'_> {
    fn dispatch(
        &mut self,
        rhandle: u64,
        method_name: &str,
        args: Vec<PluginValue>,
        _out_param_positions: &[u32],
    ) -> Result<PluginValue, PhpThrow> {
        if rhandle == 0 {
            if method_name == "__shirabe_find_file" {
                let class = arg::<String>(method_name, &args, 0)?;
                return Ok(find_file_in_registered_loaders(&class).to_plugin_value());
            }
            if method_name == "__shirabe_run_rust_command" {
                let name = arg::<String>(method_name, &args, 0)?;
                let input_line = arg::<String>(method_name, &args, 1)?;
                return match crate::console::application::run_worker_reverse_command(
                    &name,
                    &input_line,
                ) {
                    Ok(code) => Ok(code.to_plugin_value()),
                    Err(e) => Err(runtime_throw(format!("{e:#}"))),
                };
            }
            if method_name == "__shirabeConstruct" {
                return construct_entity(&args);
            }
            return Err(runtime_throw(format!(
                "unknown runtime service method `{method_name}`"
            )));
        }

        if let Some((event_rhandle, event)) = self.event
            && event_rhandle == rhandle
        {
            return dispatch_event_method(event, method_name);
        }

        dispatch_r_table_method(rhandle, method_name, &args)
    }
}

/// Serves a method call on an R-table entity, shared by every dispatcher: the child holds a
/// stub for an entity registered by an earlier call, and the handle outlives the call that
/// minted it.
pub(crate) fn dispatch_r_table_method(
    rhandle: u64,
    method_name: &str,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    // The entity is cloned out so no table borrow is held while the handler runs (a
    // handler that re-enters register_*_entity would otherwise panic on the RefCell).
    let entity = R_TABLE.with(|table| table.borrow().get(&rhandle).cloned());
    if method_name == "__shirabeClone" {
        return match entity {
            Some(entity) => clone_entity(&entity),
            None => Err(runtime_throw(format!("unknown Rust handle {rhandle}"))),
        };
    }
    if matches!(method_name, "__get" | "__set" | "__isset" | "__unset") {
        return match entity {
            Some(entity) => dispatch_property_access(&entity, method_name, args),
            None => Err(runtime_throw(format!("unknown Rust handle {rhandle}"))),
        };
    }
    match entity {
        Some(RustEntity::Io(io)) => dispatch_io_method(&io, method_name, args),
        Some(RustEntity::Composer(composer)) => dispatch_composer_method(&composer, method_name),
        Some(RustEntity::Config(config)) => dispatch_config_method(&config, method_name, args),
        Some(RustEntity::DownloadManager(dm)) => {
            dispatch_download_manager_method(&dm, method_name, args)
        }
        Some(RustEntity::Filesystem(fs)) => dispatch_filesystem_method(&fs, method_name, args),
        Some(RustEntity::InstallationManager(im)) => {
            dispatch_installation_manager_method(&im, method_name, args)
        }
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
        Some(RustEntity::Plugin(plugin)) => dispatch_plugin_method(&plugin, method_name),
        None => Err(runtime_throw(format!("unknown Rust handle {rhandle}"))),
    }
}

/// Serves the constructor every proxy stub carries: plugin code writing `new SomeProxiedClass`
/// gets a fresh Rust-side entity, since the proxied FQCN has no implementation of its own in the
/// child. Classes whose entity cannot be built here are an explicit error, so a plugin never
/// ends up holding a second, unconnected instance of a Composer service.
pub(crate) fn construct_entity(args: &[PluginValue]) -> Result<PluginValue, PhpThrow> {
    let (class, ctor_args) = match (args.first(), args.get(1)) {
        // TODO(bytes): lossy UTF-8; class names are bytes in PHP.
        (Some(PluginValue::String(class)), Some(PluginValue::List(ctor_args))) => {
            (String::from_utf8_lossy(class).into_owned(), ctor_args)
        }
        (Some(PluginValue::String(class)), None | Some(PluginValue::Array(_))) => {
            // An empty PHP array crosses as a list; anything else keyed is a protocol error.
            (String::from_utf8_lossy(class).into_owned(), &Vec::new())
        }
        other => {
            return Err(runtime_throw(format!(
                "__shirabeConstruct expects a class name and an argument list, got {other:?}"
            )));
        }
    };
    let string_arg = |position: usize| arg::<String>(&class, ctor_args, position);
    let package_arg = |position: usize| arg::<PackageInterfaceHandle>(&class, ctor_args, position);
    let alias_package_arg =
        |position: usize| -> Result<crate::package::AliasPackageHandle, PhpThrow> {
            package_arg(position)?
                .as_alias()
                .ok_or_else(|| runtime_throw(format!("{class} expects an AliasPackage")))
        };
    let package = match class.as_str() {
        "Composer\\Package\\Package" => AnyPackage::Package(crate::package::Package::new(
            string_arg(0)?,
            string_arg(1)?,
            string_arg(2)?,
        )),
        "Composer\\Package\\CompletePackage" => AnyPackage::CompletePackage(
            crate::package::CompletePackage::new(string_arg(0)?, string_arg(1)?, string_arg(2)?),
        ),
        // The alias target has to be a package that already lives on the Rust side; an alias of
        // an alias has no Rust representation, so its narrowing is an explicit error too.
        "Composer\\Package\\AliasPackage" => {
            let alias_of = package_arg(0)?
                .as_package()
                .ok_or_else(|| runtime_throw(format!("{class} expects a real Package to alias")))?;
            AnyPackage::AliasPackage(crate::package::AliasPackage::new(
                alias_of,
                string_arg(1)?,
                string_arg(2)?,
            ))
        }
        "Composer\\Package\\CompleteAliasPackage" => {
            let alias_of = package_arg(0)?.as_complete_package().ok_or_else(|| {
                runtime_throw(format!("{class} expects a real CompletePackage to alias"))
            })?;
            AnyPackage::CompleteAliasPackage(crate::package::CompleteAliasPackage::new(
                alias_of,
                string_arg(1)?,
                string_arg(2)?,
            ))
        }
        "Composer\\Package\\RootAliasPackage" => {
            let alias_of = package_arg(0)?.as_root_package().ok_or_else(|| {
                runtime_throw(format!("{class} expects a real RootPackage to alias"))
            })?;
            AnyPackage::RootAliasPackage(crate::package::RootAliasPackage::new(
                alias_of,
                string_arg(1)?,
                string_arg(2)?,
            ))
        }
        // A solver operation carries no state beyond the packages it names, so a plugin-built
        // one is a complete instance rather than a second view on a Rust-side service.
        "Composer\\DependencyResolver\\Operation\\InstallOperation" => {
            return Ok(operation_construction_result(AnyOperation::Install(
                crate::dependency_resolver::operation::InstallOperation::new(package_arg(0)?),
            )));
        }
        "Composer\\DependencyResolver\\Operation\\UpdateOperation" => {
            return Ok(operation_construction_result(AnyOperation::Update(
                crate::dependency_resolver::operation::UpdateOperation::new(
                    package_arg(0)?,
                    package_arg(1)?,
                ),
            )));
        }
        "Composer\\DependencyResolver\\Operation\\UninstallOperation" => {
            return Ok(operation_construction_result(AnyOperation::Uninstall(
                crate::dependency_resolver::operation::UninstallOperation::new(package_arg(0)?),
            )));
        }
        "Composer\\DependencyResolver\\Operation\\MarkAliasInstalledOperation" => {
            return Ok(operation_construction_result(
                AnyOperation::MarkAliasInstalled(
                    crate::dependency_resolver::operation::MarkAliasInstalledOperation::new(
                        alias_package_arg(0)?,
                    ),
                ),
            ));
        }
        "Composer\\DependencyResolver\\Operation\\MarkAliasUninstalledOperation" => {
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
        "Composer\\Util\\Filesystem" => {
            // TODO(plugin): `ProcessExecutor` has no proxy stub, so an executor argument could
            // only be a second instance the Rust side never sees.
            match ctor_args.first() {
                None | Some(PluginValue::Null) => {}
                other => {
                    return Err(runtime_throw(format!(
                        "{class} cannot take a ProcessExecutor over RPC yet, got {other:?}"
                    )));
                }
            }
            let rhandle = register_entity(RustEntity::Filesystem(std::rc::Rc::new(
                std::cell::RefCell::new(crate::util::Filesystem::new(None)),
            )));
            return Ok(construction_result(rhandle));
        }
        // TODO(plugin): the remaining proxied classes get a construction story on demand,
        // driven by explicit errors from real plugins. Each one has to decide what a
        // plugin-built instance means for the Rust-side graph, which is why none of them is
        // answered generically here.
        other => {
            return Err(runtime_throw(format!(
                "Shirabe does not support constructing {other} inside the plugin process yet"
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
        | RustEntity::Config(_)
        | RustEntity::DownloadManager(_)
        | RustEntity::Filesystem(_)
        | RustEntity::Io(_)
        | RustEntity::InstallationManager(_)
        | RustEntity::RepositoryManager(_)
        | RustEntity::Repository(_)
        | RustEntity::EventDispatcher(_)
        | RustEntity::Operation(_)
        | RustEntity::Plugin(_) => {
            return Err(runtime_throw(
                "cloning this Rust-side entity over RPC is not supported".to_string(),
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
    method_name: &str,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let property = arg::<String>(method_name, args, 0)?;
    // `BasePackage::$id` is the one instance property the entities expose so far.
    if let RustEntity::Package(package) = entity
        && property == "id"
    {
        return match method_name {
            "__get" => Ok(package
                .borrow()
                .as_package_interface()
                .get_id()
                .to_plugin_value()),
            "__isset" => Ok(PluginValue::Bool(true)),
            "__set" => {
                let id = arg::<i64>(method_name, args, 1)?;
                package.borrow_mut().as_package_interface_mut().set_id(id);
                Ok(PluginValue::Null)
            }
            _ => Err(runtime_throw(
                "the package property `id` cannot be unset over RPC".to_string(),
            )),
        };
    }
    // TODO(plugin): the instance properties the proxied classes expose are widened on demand,
    // driven by these explicit errors from real plugins. Each one has to decide how the state
    // the real class keeps in that property is served from the Rust-side entity.
    Err(runtime_throw(format!(
        "the property `{property}` is not available over RPC yet"
    )))
}

fn dispatch_plugin_method(
    plugin: &std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
    method_name: &str,
) -> Result<PluginValue, PhpThrow> {
    match method_name {
        "getCapabilities" => {
            let plugin = plugin.borrow();
            let capable = plugin.as_capable().ok_or_else(|| {
                runtime_throw(format!(
                    "plugin {} does not implement Capable",
                    plugin.get_class_name()
                ))
            })?;
            let capabilities = capable
                .get_capabilities()
                .map_err(|error| runtime_throw(format!("getCapabilities failed: {error:#}")))?;
            Ok(capabilities.to_plugin_value())
        }
        // TODO(plugin): the lifecycle methods would have to turn the `$composer`/`$io` stubs the
        // child passes back into the Rust handles they stand for; nothing calls them, because
        // Composer activates a Rust-implemented plugin on the Rust side.
        other => Err(runtime_throw(format!(
            "the plugin method `{other}` is not available over RPC yet"
        ))),
    }
}

fn dispatch_composer_method(
    composer: &ComposerHandle,
    method_name: &str,
) -> Result<PluginValue, PhpThrow> {
    match method_name {
        "getRepositoryManager" => {
            let rm = composer.borrow().get_repository_manager();
            let rhandle = register_entity(RustEntity::RepositoryManager(rm));
            Ok(rust_handle_value(
                rhandle,
                "Composer\\Repository\\RepositoryManager",
            ))
        }
        "getInstallationManager" => {
            let im = composer.borrow().get_installation_manager();
            let rhandle = register_entity(RustEntity::InstallationManager(im));
            Ok(rust_handle_value(
                rhandle,
                "Composer\\Installer\\InstallationManager",
            ))
        }
        "getPackage" => {
            let package = composer.borrow().get_package().as_rc().clone();
            Ok(package_handle_value(&package))
        }
        "getEventDispatcher" => {
            let dispatcher = composer.borrow().get_event_dispatcher();
            let rhandle = register_entity(RustEntity::EventDispatcher(dispatcher));
            Ok(rust_handle_value(
                rhandle,
                "Composer\\EventDispatcher\\EventDispatcher",
            ))
        }
        "getConfig" => {
            let config = composer.borrow().get_config();
            let rhandle = register_entity(RustEntity::Config(config));
            Ok(rust_handle_value(rhandle, "Composer\\Config"))
        }
        "getDownloadManager" => {
            let dm = composer.borrow().get_download_manager();
            let rhandle = register_entity(RustEntity::DownloadManager(dm));
            Ok(rust_handle_value(
                rhandle,
                "Composer\\Downloader\\DownloadManager",
            ))
        }
        // TODO(plugin): the remaining Composer object graph (getLocker, getPluginManager, ...)
        // becomes reachable over RPC on demand, driven by explicit errors from real plugins.
        other => Err(runtime_throw(format!(
            "the Composer method `{other}` is not available over RPC yet"
        ))),
    }
}

fn dispatch_config_method(
    config: &std::rc::Rc<std::cell::RefCell<crate::config::Config>>,
    method_name: &str,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let key = |position: usize| arg::<String>(method_name, args, position);
    let flags = |position: usize| arg_or::<i64>(method_name, args, position, 0);
    match method_name {
        "get" => {
            let value = config
                .borrow()
                .get_with_flags(&key(0)?, flags(1)?)
                .map_err(|error| runtime_throw(format!("get failed over RPC: {error}")))?;
            Ok(value.to_plugin_value())
        }
        "all" => {
            let all = config
                .borrow_mut()
                .all(flags(0)?)
                .map_err(|error| runtime_throw(format!("all failed over RPC: {error}")))?;
            Ok(all.to_plugin_value())
        }
        "raw" => Ok(config.borrow().raw().to_plugin_value()),
        "has" => Ok(config.borrow().has(&key(0)?).to_plugin_value()),
        "getRepositories" => Ok(config.borrow().get_repositories().to_plugin_value()),
        "getSourceOfValue" => Ok(config
            .borrow_mut()
            .get_source_of_value(&key(0)?)
            .to_plugin_value()),
        "merge" => {
            let values = arg::<IndexMap<String, PhpMixed>>(method_name, args, 0)?;
            let source = arg_or::<String>(
                method_name,
                args,
                1,
                crate::config::Config::SOURCE_UNKNOWN.to_string(),
            )?;
            config.borrow_mut().merge(&values, &source);
            Ok(PluginValue::Null)
        }
        // TODO(plugin): the config-source surface (getConfigSource / setAuthConfigSource / ...)
        // needs proxy stubs for ConfigSourceInterface implementations, and prohibitUrlByConfig
        // needs the IO argument decoded back into the Rust-side instance; both are widened on
        // demand, driven by explicit errors from real plugins.
        other => Err(runtime_throw(format!(
            "the Config method `{other}` is not available over RPC yet"
        ))),
    }
}

/// The download manager's contract is asynchronous on both sides: PHP declares a
/// `PromiseInterface` return, Rust an `async fn`. The Rust future is driven to completion here
/// and its value handed back as an already-settled React promise.
///
/// TODO(async): the boundary has no representation for a promise that is still pending, so the
/// deferred resolution the PHP contract allows collapses into a blocking wait here.
fn dispatch_download_manager_method(
    dm: &std::rc::Rc<std::cell::RefCell<dyn crate::downloader::DownloadManagerInterface>>,
    method_name: &str,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let string_arg = |position: usize| arg::<String>(method_name, args, position);
    match method_name {
        "setPreferSource" | "setPreferDist" => {
            let preferred = arg::<bool>(method_name, args, 0)?;
            if method_name == "setPreferSource" {
                dm.borrow_mut().set_prefer_source(preferred);
            } else {
                dm.borrow_mut().set_prefer_dist(preferred);
            }
            Ok(PluginValue::Null)
        }
        "download" | "prepare" | "install" | "update" | "remove" | "cleanup" => {
            let resolved = crate::util::sync_executor::block_on(async {
                let dm = dm.borrow();
                match method_name {
                    "download" => {
                        dm.download(
                            arg::<PackageInterfaceHandle>(method_name, args, 0)?,
                            &string_arg(1)?,
                            arg::<Option<PackageInterfaceHandle>>(method_name, args, 2)?,
                        )
                        .await
                    }
                    "prepare" => {
                        dm.prepare(
                            &string_arg(0)?,
                            arg::<PackageInterfaceHandle>(method_name, args, 1)?,
                            &string_arg(2)?,
                            arg::<Option<PackageInterfaceHandle>>(method_name, args, 3)?,
                        )
                        .await
                    }
                    "install" => {
                        dm.install(
                            arg::<PackageInterfaceHandle>(method_name, args, 0)?,
                            &string_arg(1)?,
                        )
                        .await
                    }
                    "update" => {
                        dm.update(
                            arg::<PackageInterfaceHandle>(method_name, args, 0)?,
                            arg::<PackageInterfaceHandle>(method_name, args, 1)?,
                            &string_arg(2)?,
                        )
                        .await
                    }
                    "remove" => {
                        dm.remove(
                            arg::<PackageInterfaceHandle>(method_name, args, 0)?,
                            &string_arg(1)?,
                        )
                        .await
                    }
                    _ => {
                        dm.cleanup(
                            &string_arg(0)?,
                            arg::<PackageInterfaceHandle>(method_name, args, 1)?,
                            &string_arg(2)?,
                            arg::<Option<PackageInterfaceHandle>>(method_name, args, 3)?,
                        )
                        .await
                    }
                }
                .map_err(|error| {
                    // TODO(plugin): the original exception class is collapsed to
                    // RuntimeException on this side of the boundary.
                    runtime_throw(format!("{method_name} failed over RPC: {error:#}"))
                })
            })?;
            resolved_promise(resolved.to_plugin_value())
        }
        // TODO(plugin): the downloader-facing surface (getDownloader / setDownloader /
        // getDownloaderForPackage / getDownloaderType) needs proxy stubs for
        // DownloaderInterface implementations before it can cross.
        other => Err(runtime_throw(format!(
            "the DownloadManager method `{other}` is not available over RPC yet"
        ))),
    }
}

/// A `\React\Promise\PromiseInterface` already fulfilled with `value`, built in the worker so
/// PHP callers get the object type their signatures declare.
fn resolved_promise(value: PluginValue) -> Result<PluginValue, PhpThrow> {
    match call_function_with_dispatcher(
        "__shirabe_resolved_promise",
        vec![value],
        Some(&mut PluginRpcDispatcher::default()),
    ) {
        Ok(Ok(promise)) => Ok(promise),
        Ok(Err(throw)) => Err(throw),
        Err(error) => Err(runtime_throw(format!(
            "creating a resolved promise in the plugin process failed: {error:#}"
        ))),
    }
}

fn dispatch_filesystem_method(
    fs: &std::rc::Rc<std::cell::RefCell<crate::util::Filesystem>>,
    method_name: &str,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let string_arg = |position: usize| arg::<String>(method_name, args, position);
    // TODO(plugin): the exception class the real method throws (RuntimeException, IOException,
    // LogicException) is collapsed to RuntimeException on this side of the boundary.
    let failed = |error: anyhow::Error| runtime_throw(format!("{method_name} failed: {error:#}"));
    match method_name {
        "remove" => Ok(fs
            .borrow_mut()
            .remove(string_arg(0)?)
            .map_err(failed)?
            .to_plugin_value()),
        "isDirEmpty" => Ok(fs.borrow().is_dir_empty(&string_arg(0)?).to_plugin_value()),
        "emptyDirectory" => {
            fs.borrow_mut()
                .empty_directory(&string_arg(0)?, arg::<bool>(method_name, args, 1)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        "removeDirectory" => Ok(fs
            .borrow_mut()
            .remove_directory(string_arg(0)?)
            .map_err(failed)?
            .to_plugin_value()),
        "removeDirectoryAsync" => {
            let directory = string_arg(0)?;
            let removed = crate::util::sync_executor::block_on(async {
                crate::util::Filesystem::remove_directory_async_via(fs, &directory).await
            })
            .map_err(failed)?;
            resolved_promise(removed.to_plugin_value())
        }
        "removeDirectoryPhp" => Ok(fs
            .borrow_mut()
            .remove_directory_php(&string_arg(0)?)
            .map_err(failed)?
            .to_plugin_value()),
        "ensureDirectoryExists" => {
            fs.borrow_mut()
                .ensure_directory_exists(&string_arg(0)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        "unlink" => Ok(fs
            .borrow()
            .unlink(string_arg(0)?)
            .map_err(failed)?
            .to_plugin_value()),
        "rmdir" => Ok(fs
            .borrow()
            .rmdir(string_arg(0)?)
            .map_err(failed)?
            .to_plugin_value()),
        "copyThenRemove" => {
            fs.borrow_mut()
                .copy_then_remove(&string_arg(0)?, &string_arg(1)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        "copy" => Ok(fs
            .borrow_mut()
            .copy(&string_arg(0)?, &string_arg(1)?)
            .map_err(failed)?
            .to_plugin_value()),
        "rename" => {
            fs.borrow_mut()
                .rename(string_arg(0)?, string_arg(1)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        "findShortestPath" | "findShortestPathCode" => {
            let from = string_arg(0)?;
            let to = string_arg(1)?;
            // TODO(error-model): the port panics on a relative path where PHP throws
            // InvalidArgumentException, and a panic would take the whole process down instead
            // of reaching the plugin's catch block, so the check is repeated here.
            let fs = fs.borrow();
            if !fs.is_absolute_path(&from) || !fs.is_absolute_path(&to) {
                return Err(PhpThrow {
                    exception_class: "InvalidArgumentException".to_string(),
                    message: format!("$from ({from}) and $to ({to}) must be absolute paths."),
                    code: 0,
                });
            }
            Ok(if method_name == "findShortestPath" {
                fs.find_shortest_path(
                    &from,
                    &to,
                    arg::<bool>(method_name, args, 2)?,
                    arg::<bool>(method_name, args, 3)?,
                )
            } else {
                fs.find_shortest_path_code(
                    &from,
                    &to,
                    arg::<bool>(method_name, args, 2)?,
                    arg::<bool>(method_name, args, 3)?,
                    arg::<bool>(method_name, args, 4)?,
                )
            }
            .to_plugin_value())
        }
        "isAbsolutePath" => Ok(fs
            .borrow()
            .is_absolute_path(&string_arg(0)?)
            .to_plugin_value()),
        "size" => Ok(fs
            .borrow()
            .size(string_arg(0)?)
            .map_err(failed)?
            .to_plugin_value()),
        "normalizePath" => Ok(fs
            .borrow()
            .normalize_path(&string_arg(0)?)
            .to_plugin_value()),
        "relativeSymlink" => Ok(fs
            .borrow()
            .relative_symlink(&string_arg(0)?, &string_arg(1)?)
            .to_plugin_value()),
        "isSymlinkedDirectory" => Ok(fs
            .borrow()
            .is_symlinked_directory(&string_arg(0)?)
            .to_plugin_value()),
        "junction" => {
            fs.borrow_mut()
                .junction(&string_arg(0)?, &string_arg(1)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        "isJunction" => Ok(fs.borrow().is_junction(&string_arg(0)?).to_plugin_value()),
        "removeJunction" => Ok(fs
            .borrow_mut()
            .remove_junction(&string_arg(0)?)
            .map_err(failed)?
            .to_plugin_value()),
        "filePutContentsIfModified" => Ok(fs
            .borrow()
            .file_put_contents_if_modified(&string_arg(0)?, &string_arg(1)?)
            .map_err(failed)?
            .to_plugin_value()),
        "safeCopy" => {
            fs.borrow()
                .safe_copy(&string_arg(0)?, &string_arg(1)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        other => Err(runtime_throw(format!(
            "the Filesystem method `{other}` is not available over RPC yet"
        ))),
    }
}

fn dispatch_event_dispatcher_method(
    dispatcher: &std::rc::Rc<
        std::cell::RefCell<dyn crate::event_dispatcher::EventDispatcherInterface>,
    >,
    method_name: &str,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name {
        "dispatch" => {
            let name = arg::<String>(method_name, args, 0)?;
            let probe = crate::event_dispatcher::Event::from_name(name.clone());
            if dispatcher.borrow_mut().has_event_listeners(&probe) {
                // TODO(plugin): dispatching a worker-constructed event through the Rust-side
                // dispatcher needs the event object (and the console input it carries) proxied
                // back into this process, so only the no-listener case is answered here.
                return Err(runtime_throw(format!(
                    "dispatching `{name}` from the plugin process is not supported yet while listeners are registered for it"
                )));
            }
            // TODO(plugin): answering 0 here skips what `do_dispatch` does before it reaches the
            // listener loop, and upstream does both regardless of the listener count: the
            // `COMPOSER_DEBUG_EVENTS` trace line, and `push_event`'s circular-call detection
            // (a nested dispatch of the same event name throws there even with no listeners).
            Ok(PluginValue::Int(0))
        }
        other => Err(runtime_throw(format!(
            "the EventDispatcher method `{other}` is not available over RPC yet"
        ))),
    }
}

fn dispatch_repository_manager_method(
    rm: &std::rc::Rc<std::cell::RefCell<dyn RepositoryManagerInterface>>,
    method_name: &str,
) -> Result<PluginValue, PhpThrow> {
    match method_name {
        "getLocalRepository" => repository_handle_value(&rm.borrow().get_local_repository()),
        // TODO(plugin): the remaining RepositoryManager surface is widened on demand, driven
        // by explicit errors from real plugins.
        other => Err(runtime_throw(format!(
            "the RepositoryManager method `{other}` is not available over RPC yet"
        ))),
    }
}

fn dispatch_repository_method(
    repository: &RepositoryInterfaceHandle,
    method_name: &str,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name {
        "hasPackage" => {
            let package = arg::<PackageInterfaceHandle>(method_name, args, 0)?;
            let has = repository.has_package(package).map_err(|error| {
                // TODO(plugin): the original exception class is collapsed to RuntimeException
                // on this side of the boundary.
                runtime_throw(format!("hasPackage failed over RPC: {error}"))
            })?;
            Ok(has.to_plugin_value())
        }
        "addPackage" | "removePackage" => {
            let package = arg::<PackageInterfaceHandle>(method_name, args, 0)?;
            let mut borrowed = repository.borrow_mut();
            let writable = borrowed
                .as_writable_repository_interface_mut()
                .ok_or_else(|| {
                    runtime_throw(format!(
                        "the repository behind this handle is not writable, so `{method_name}` cannot be called on it"
                    ))
                })?;
            let outcome = if method_name == "addPackage" {
                writable.add_package(package)
            } else {
                writable.remove_package(package)
            };
            outcome.map_err(|error| {
                // TODO(plugin): the original exception class is collapsed to RuntimeException
                // on this side of the boundary.
                runtime_throw(format!("{method_name} failed over RPC: {error}"))
            })?;
            Ok(PluginValue::Null)
        }
        "getPackages" => {
            let packages = repository.borrow_mut().get_packages().map_err(|error| {
                // TODO(plugin): the original exception class is collapsed to RuntimeException
                // on this side of the boundary.
                runtime_throw(format!("getPackages failed over RPC: {error}"))
            })?;
            let mut items = Vec::with_capacity(packages.len());
            for package in packages {
                items.push(package_handle_value(package.as_rc()));
            }
            Ok(PluginValue::List(items))
        }
        // TODO(plugin): the remaining RepositoryInterface surface is widened on demand,
        // driven by explicit errors from real plugins.
        other => Err(runtime_throw(format!(
            "the repository method `{other}` is not available over RPC yet"
        ))),
    }
}

/// Decodes one positional argument of an RPC method call. A missing argument decodes the same
/// way PHP's `null` does; a parameter's own default belongs to `arg_or`.
trait FromPluginArg: Sized {
    fn from_arg(
        method: &str,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow>;
}

/// Encodes a Rust value as the PHP value the calling proxy stub declares.
trait ToPluginValue {
    fn to_plugin_value(self) -> PluginValue;
}

/// Decodes the argument at `position`.
fn arg<T: FromPluginArg>(
    method: &str,
    args: &[PluginValue],
    position: usize,
) -> Result<T, PhpThrow> {
    T::from_arg(method, position, args.get(position))
}

/// Decodes the argument at `position` for a parameter whose PHP declaration carries a default:
/// an omitted argument, and the `null` PHP passes in its place, take `default`.
fn arg_or<T: FromPluginArg>(
    method: &str,
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
    method: &str,
    position: usize,
    expected: &str,
    value: Option<&PluginValue>,
) -> PhpThrow {
    runtime_throw(format!(
        "{method} expects {expected} at position {position}, got {value:?}"
    ))
}

impl FromPluginArg for bool {
    fn from_arg(
        method: &str,
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
        method: &str,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(PluginValue::Int(value)) => Ok(*value),
            other => Err(arg_throw(method, position, "an int", other)),
        }
    }
}

impl FromPluginArg for String {
    fn from_arg(
        method: &str,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            // TODO(bytes): lossy UTF-8; every PHP string crossing the boundary is bytes.
            Some(PluginValue::String(bytes)) => Ok(String::from_utf8_lossy(bytes).into_owned()),
            other => Err(arg_throw(method, position, "a string", other)),
        }
    }
}

impl FromPluginArg for PhpMixed {
    fn from_arg(
        method: &str,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            None => Ok(PhpMixed::Null),
            Some(value) => value.to_php_mixed().map_err(|error| {
                runtime_throw(format!(
                    "{method} could not decode its argument at position {position}: {error:#}"
                ))
            }),
        }
    }
}

impl<T: FromPluginArg> FromPluginArg for Option<T> {
    fn from_arg(
        method: &str,
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
        method: &str,
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
impl<T: FromPluginArg> FromPluginArg for IndexMap<String, T> {
    fn from_arg(
        method: &str,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        let entries: Vec<(String, &PluginValue)> = match value {
            Some(PluginValue::Array(map)) => map
                .iter()
                // TODO(bytes): lossy UTF-8; PHP array keys are bytes.
                .map(|(key, item)| (String::from_utf8_lossy(key).into_owned(), item))
                .collect(),
            Some(PluginValue::List(items)) => items
                .iter()
                .enumerate()
                .map(|(index, item)| (index.to_string(), item))
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
        method: &str,
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
        method: &str,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        let row = match value {
            Some(PluginValue::Array(row)) => row,
            other => return Err(arg_throw(method, position, "a mirror map", other)),
        };
        let url = match row.get(b"url".as_slice()) {
            // TODO(bytes): lossy UTF-8; PHP strings are bytes.
            Some(PluginValue::String(url)) => String::from_utf8_lossy(url).into_owned(),
            other => {
                return Err(runtime_throw(format!(
                    "{method} expects a string `url` in every mirror, got {other:?}"
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
        method: &str,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(PluginValue::RustHandle(handle)) => {
                match R_TABLE.with(|table| table.borrow().get(&handle.rhandle).cloned()) {
                    Some(RustEntity::Package(package)) => {
                        Ok(PackageInterfaceHandle::from_rc_unchecked(package))
                    }
                    _ => Err(runtime_throw(format!(
                        "{method} expects a package handle, got Rust handle {}",
                        handle.rhandle
                    ))),
                }
            }
            other => Err(arg_throw(method, position, "a package", other)),
        }
    }
}

/// Resolves a repository argument back to the Rust-side entity its proxy stub stands for.
impl FromPluginArg for RepositoryInterfaceHandle {
    fn from_arg(
        method: &str,
        position: usize,
        value: Option<&PluginValue>,
    ) -> Result<Self, PhpThrow> {
        match value {
            Some(PluginValue::RustHandle(handle)) => {
                match R_TABLE.with(|table| table.borrow().get(&handle.rhandle).cloned()) {
                    Some(RustEntity::Repository(repository)) => Ok(repository),
                    _ => Err(runtime_throw(format!(
                        "{method} expects a repository handle, got Rust handle {}",
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
        method: &str,
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
        method: &str,
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
        method: &str,
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

impl ToPluginValue for String {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::string(self)
    }
}

impl ToPluginValue for &str {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::string(self)
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
impl<T: ToPluginValue> ToPluginValue for IndexMap<String, T> {
    fn to_plugin_value(self) -> PluginValue {
        if self.is_empty() {
            PluginValue::List(Vec::new())
        } else {
            PluginValue::Array(
                self.into_iter()
                    .map(|(key, value)| (key.into_bytes(), value.to_plugin_value()))
                    .collect(),
            )
        }
    }
}

/// `array{url: string, preferred: bool}` as PHP shapes it.
impl ToPluginValue for crate::package::Mirror {
    fn to_plugin_value(self) -> PluginValue {
        PluginValue::Array(IndexMap::from([
            (b"url".to_vec(), PluginValue::string(self.url)),
            (b"preferred".to_vec(), PluginValue::Bool(self.preferred)),
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

impl ToPluginValue for chrono::DateTime<chrono::Utc> {
    fn to_plugin_value(self) -> PluginValue {
        date_time_to_wire(&self)
    }
}

/// The read half of the `CompletePackage` / `RootPackage` surface, which `PackageInterface`
/// does not carry. `None` means the method belongs to the base surface below.
fn dispatch_complete_package_getter(
    package: &AnyPackage,
    method_name: &str,
) -> Result<Option<PluginValue>, PhpThrow> {
    let unavailable = || {
        runtime_throw(format!(
            "`{method_name}` is not available on this package over RPC"
        ))
    };
    let value = match method_name {
        "getScripts"
        | "getRepositories"
        | "getLicense"
        | "getKeywords"
        | "getDescription"
        | "getHomepage"
        | "getAuthors"
        | "getSupport"
        | "getFunding"
        | "isAbandoned"
        | "getReplacementPackage"
        | "getArchiveName"
        | "getArchiveExcludes" => {
            let package = package
                .as_complete_package_interface()
                .ok_or_else(unavailable)?;
            match method_name {
                "getScripts" => package.get_scripts().to_plugin_value(),
                "getRepositories" => package.get_repositories().to_plugin_value(),
                "getLicense" => package.get_license().to_plugin_value(),
                "getKeywords" => package.get_keywords().to_plugin_value(),
                "getDescription" => package.get_description().to_plugin_value(),
                "getHomepage" => package.get_homepage().to_plugin_value(),
                "getAuthors" => package.get_authors().to_plugin_value(),
                "getSupport" => package.get_support().to_plugin_value(),
                "getFunding" => package.get_funding().to_plugin_value(),
                "isAbandoned" => PluginValue::Bool(package.is_abandoned()),
                "getReplacementPackage" => package.get_replacement_package().to_plugin_value(),
                "getArchiveName" => package.get_archive_name().to_plugin_value(),
                _ => package.get_archive_excludes().to_plugin_value(),
            }
        }
        "getAliases"
        | "getMinimumStability"
        | "getStabilityFlags"
        | "getReferences"
        | "getPreferStable"
        | "getConfig" => {
            let package = package
                .as_root_package_interface()
                .ok_or_else(unavailable)?;
            match method_name {
                "getAliases" => package.get_aliases().to_plugin_value(),
                "getMinimumStability" => PluginValue::string(package.get_minimum_stability()),
                "getStabilityFlags" => package.get_stability_flags().to_plugin_value(),
                "getReferences" => package.get_references().to_plugin_value(),
                "getPreferStable" => PluginValue::Bool(package.get_prefer_stable()),
                _ => package.get_config().to_plugin_value(),
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}

fn dispatch_package_method(
    package: &std::rc::Rc<std::cell::RefCell<AnyPackage>>,
    method_name: &str,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    // Mutators borrow mutably and must not hold the borrow across the shared-borrow arms.
    match method_name {
        "setId" => {
            let id = arg::<i64>(method_name, args, 0)?;
            package.borrow_mut().as_package_interface_mut().set_id(id);
            return Ok(PluginValue::Null);
        }
        "setInstallationSource"
        | "setSourceReference"
        | "setSourceUrl"
        | "setDistUrl"
        | "setDistType"
        | "setDistReference"
        | "setSourceDistReferences" => {
            let value = arg::<Option<String>>(method_name, args, 0)?;
            let mut borrowed = package.borrow_mut();
            let package = borrowed.as_package_interface_mut();
            match method_name {
                "setInstallationSource" => package.set_installation_source(value),
                "setSourceReference" => package.set_source_reference(value),
                "setSourceUrl" => package.set_source_url(value),
                "setDistUrl" => package.set_dist_url(value),
                "setDistType" => package.set_dist_type(value),
                "setDistReference" => package.set_dist_reference(value),
                _ => package.set_source_dist_references(value.ok_or_else(|| {
                    runtime_throw("setSourceDistReferences expects a reference string".to_string())
                })?),
            }
            return Ok(PluginValue::Null);
        }
        "setSourceMirrors" | "setDistMirrors" => {
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
        "setRepository" => {
            let repository = arg::<RepositoryInterfaceHandle>(method_name, args, 0)?;
            package
                .borrow_mut()
                .as_package_interface_mut()
                .set_repository(repository)
                .map_err(|error| runtime_throw(format!("setRepository failed: {error}")))?;
            return Ok(PluginValue::Null);
        }
        "setTransportOptions" => {
            let options =
                arg_or::<IndexMap<String, PhpMixed>>(method_name, args, 0, IndexMap::new())?;
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
        "setRequires" | "setDevRequires" | "setConflicts" | "setProvides" | "setReplaces"
        | "setAutoload" | "setDevAutoload" | "setSuggests" | "setExtra"
            if package.borrow().is_root() =>
        {
            let mut borrowed = package.borrow_mut();
            let package = borrowed
                .as_root_package_interface_mut()
                .expect("a root package exposes RootPackageInterface");
            match method_name {
                "setRequires" => package.set_requires(arg_or::<
                    IndexMap<String, crate::package::Link>,
                >(
                    method_name, args, 0, IndexMap::new()
                )?),
                "setDevRequires" => {
                    package.set_dev_requires(arg_or::<IndexMap<String, crate::package::Link>>(
                        method_name,
                        args,
                        0,
                        IndexMap::new(),
                    )?)
                }
                "setConflicts" => package.set_conflicts(arg_or::<
                    IndexMap<String, crate::package::Link>,
                >(
                    method_name, args, 0, IndexMap::new()
                )?),
                "setProvides" => package.set_provides(arg_or::<
                    IndexMap<String, crate::package::Link>,
                >(
                    method_name, args, 0, IndexMap::new()
                )?),
                "setReplaces" => package.set_replaces(arg_or::<
                    IndexMap<String, crate::package::Link>,
                >(
                    method_name, args, 0, IndexMap::new()
                )?),
                "setAutoload" => {
                    package.set_autoload(arg::<IndexMap<String, PhpMixed>>(method_name, args, 0)?)
                }
                "setDevAutoload" => package.set_dev_autoload(arg::<IndexMap<String, PhpMixed>>(
                    method_name,
                    args,
                    0,
                )?),
                "setSuggests" => {
                    package.set_suggests(arg::<IndexMap<String, String>>(method_name, args, 0)?)
                }
                _ => package.set_extra(arg::<IndexMap<String, PhpMixed>>(method_name, args, 0)?),
            }
            return Ok(PluginValue::Null);
        }
        // `Package`'s own setters. The concrete subclasses inherit them (their PHP overrides in
        // `RootPackage` delegate to the same base state), so the base package answers for every
        // real variant.
        "setType"
        | "setTargetDir"
        | "setExtra"
        | "setBinaries"
        | "setSourceType"
        | "setDistSha1Checksum"
        | "setSuggests"
        | "setAutoload"
        | "setDevAutoload"
        | "setIncludePaths"
        | "setPhpExt"
        | "setNotificationUrl"
        | "setIsDefaultBranch"
        | "replaceVersion" => {
            let mut borrowed = package.borrow_mut();
            let package = borrowed.as_package_mut().ok_or_else(|| {
                runtime_throw(format!(
                    "`{method_name}` is not available on an alias package over RPC"
                ))
            })?;
            match method_name {
                "setType" => package.set_type(arg::<String>(method_name, args, 0)?),
                "setTargetDir" => {
                    package.set_target_dir(arg::<Option<String>>(method_name, args, 0)?)
                }
                "setExtra" => {
                    package.set_extra(arg::<IndexMap<String, PhpMixed>>(method_name, args, 0)?)
                }
                "setBinaries" => package.set_binaries(arg::<Vec<String>>(method_name, args, 0)?),
                "setSourceType" => {
                    package.set_source_type(arg::<Option<String>>(method_name, args, 0)?)
                }
                "setDistSha1Checksum" => {
                    package.set_dist_sha1_checksum(arg::<Option<String>>(method_name, args, 0)?)
                }
                "setSuggests" => {
                    package.set_suggests(arg::<IndexMap<String, String>>(method_name, args, 0)?)
                }
                "setAutoload" => {
                    package.set_autoload(arg::<IndexMap<String, PhpMixed>>(method_name, args, 0)?)
                }
                "setDevAutoload" => package.set_dev_autoload(arg::<IndexMap<String, PhpMixed>>(
                    method_name,
                    args,
                    0,
                )?),
                "setIncludePaths" => {
                    package.set_include_paths(arg::<Vec<String>>(method_name, args, 0)?)
                }
                "setPhpExt" => package.set_php_ext(arg::<Option<IndexMap<String, PhpMixed>>>(
                    method_name,
                    args,
                    0,
                )?),
                "setNotificationUrl" => {
                    package.set_notification_url(arg::<String>(method_name, args, 0)?)
                }
                "setIsDefaultBranch" => {
                    package.set_is_default_branch(arg::<bool>(method_name, args, 0)?)
                }
                _ => package.replace_version(
                    arg::<String>(method_name, args, 0)?,
                    arg::<String>(method_name, args, 1)?,
                ),
            }
            return Ok(PluginValue::Null);
        }
        "setRequires" | "setConflicts" | "setProvides" | "setReplaces" | "setDevRequires" => {
            let links = arg_or::<IndexMap<String, crate::package::Link>>(
                method_name,
                args,
                0,
                IndexMap::new(),
            )?;
            let mut borrowed = package.borrow_mut();
            let package = borrowed.as_package_mut().ok_or_else(|| {
                runtime_throw(format!(
                    "`{method_name}` is not available on an alias package over RPC"
                ))
            })?;
            match method_name {
                "setRequires" => package.set_requires(links),
                "setConflicts" => package.set_conflicts(links),
                "setProvides" => package.set_provides(links),
                "setReplaces" => package.set_replaces(links),
                _ => package.set_dev_requires(links),
            }
            return Ok(PluginValue::Null);
        }
        "setReleaseDate" => {
            let date = arg::<Option<chrono::DateTime<chrono::Utc>>>(method_name, args, 0)?;
            let mut borrowed = package.borrow_mut();
            let package = borrowed.as_package_mut().ok_or_else(|| {
                runtime_throw(
                    "`setReleaseDate` is not available on an alias package over RPC".to_string(),
                )
            })?;
            package.set_release_date(date);
            return Ok(PluginValue::Null);
        }
        "setScripts" | "setRepositories" | "setLicense" | "setKeywords" | "setDescription"
        | "setHomepage" | "setAuthors" | "setSupport" | "setFunding" | "setAbandoned"
        | "setArchiveName" | "setArchiveExcludes" => {
            let mut borrowed = package.borrow_mut();
            let package = borrowed
                .as_complete_package_interface_mut()
                .ok_or_else(|| {
                    runtime_throw(format!(
                        "`{method_name}` is not available on this package over RPC"
                    ))
                })?;
            match method_name {
                "setScripts" => {
                    package.set_scripts(arg::<IndexMap<String, Vec<String>>>(method_name, args, 0)?)
                }
                "setRepositories" => package.set_repositories(arg::<IndexMap<String, PhpMixed>>(
                    method_name,
                    args,
                    0,
                )?),
                "setLicense" => package.set_license(arg::<Vec<String>>(method_name, args, 0)?),
                "setKeywords" => package.set_keywords(arg::<Vec<String>>(method_name, args, 0)?),
                "setDescription" => package.set_description(arg::<String>(method_name, args, 0)?),
                "setHomepage" => package.set_homepage(arg::<String>(method_name, args, 0)?),
                "setAuthors" => {
                    package.set_authors(arg::<Vec<IndexMap<String, String>>>(method_name, args, 0)?)
                }
                "setSupport" => {
                    package.set_support(arg::<IndexMap<String, String>>(method_name, args, 0)?)
                }
                "setFunding" => package.set_funding(arg::<Vec<IndexMap<String, PhpMixed>>>(
                    method_name,
                    args,
                    0,
                )?),
                "setAbandoned" => package.set_abandoned(arg::<PhpMixed>(method_name, args, 0)?),
                "setArchiveName" => package.set_archive_name(arg::<String>(method_name, args, 0)?),
                _ => package.set_archive_excludes(arg::<Vec<String>>(method_name, args, 0)?),
            }
            return Ok(PluginValue::Null);
        }
        "setStabilityFlags"
        | "setMinimumStability"
        | "setPreferStable"
        | "setConfig"
        | "setReferences"
        | "setAliases" => {
            let mut borrowed = package.borrow_mut();
            let package = borrowed.as_root_package_interface_mut().ok_or_else(|| {
                runtime_throw(format!(
                    "`{method_name}` is not available on this package over RPC"
                ))
            })?;
            match method_name {
                "setStabilityFlags" => {
                    package.set_stability_flags(arg::<IndexMap<String, i64>>(method_name, args, 0)?)
                }
                "setMinimumStability" => {
                    package.set_minimum_stability(arg::<String>(method_name, args, 0)?)
                }
                "setPreferStable" => package.set_prefer_stable(arg::<bool>(method_name, args, 0)?),
                "setConfig" => {
                    package.set_config(arg::<IndexMap<String, PhpMixed>>(method_name, args, 0)?)
                }
                "setReferences" => {
                    package.set_references(arg::<IndexMap<String, String>>(method_name, args, 0)?)
                }
                _ => {
                    package.set_aliases(arg::<Vec<IndexMap<String, String>>>(method_name, args, 0)?)
                }
            }
            return Ok(PluginValue::Null);
        }
        "equals" => {
            let other = arg::<PackageInterfaceHandle>(method_name, args, 0)?;
            let this = PackageInterfaceHandle::from_rc_unchecked(package.clone());
            return Ok(this.equals(&other).to_plugin_value());
        }
        // The subclasses narrow `getAliasOf`'s return type to their own alias target, but every
        // variant holds the one entity.
        "getAliasOf" | "isRootPackageAlias" | "hasSelfVersionRequires" => {
            let borrowed = package.borrow();
            let alias = borrowed.as_alias_package().ok_or_else(|| {
                runtime_throw(format!(
                    "`{method_name}` is not available on this package over RPC"
                ))
            })?;
            return Ok(match method_name {
                "getAliasOf" => package_handle_value(alias.get_alias_of().as_rc()),
                "isRootPackageAlias" => alias.is_root_package_alias().to_plugin_value(),
                _ => alias.has_self_version_requires().to_plugin_value(),
            });
        }
        "setRootPackageAlias" => {
            let value = arg::<bool>(method_name, args, 0)?;
            package
                .borrow_mut()
                .as_alias_package_mut()
                .ok_or_else(|| {
                    runtime_throw(
                        "`setRootPackageAlias` is not available on this package over RPC"
                            .to_string(),
                    )
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
    match method_name {
        "getName" => Ok(package.get_name().to_string().to_plugin_value()),
        "getPrettyName" => Ok(package.get_pretty_name().to_string().to_plugin_value()),
        "getNames" => {
            let provides = arg_or::<bool>(method_name, args, 0, true)?;
            Ok(package.get_names(provides).to_plugin_value())
        }
        "getId" => Ok(package.get_id().to_plugin_value()),
        "isDev" => Ok(package.is_dev().to_plugin_value()),
        "getType" => Ok(package.get_type().to_plugin_value()),
        "getTargetDir" => Ok(package.get_target_dir().to_plugin_value()),
        "getExtra" => Ok(package.get_extra().to_plugin_value()),
        "getInstallationSource" => Ok(package.get_installation_source().to_plugin_value()),
        "getSourceType" => Ok(package.get_source_type().to_plugin_value()),
        "getSourceUrl" => Ok(package.get_source_url().to_plugin_value()),
        "getSourceUrls" => Ok(package.get_source_urls().to_plugin_value()),
        "getSourceReference" => Ok(package.get_source_reference().to_plugin_value()),
        "getSourceMirrors" => Ok(package.get_source_mirrors().to_plugin_value()),
        "getDistType" => Ok(package.get_dist_type().to_plugin_value()),
        "getDistUrl" => Ok(package.get_dist_url().to_plugin_value()),
        "getDistUrls" => Ok(package.get_dist_urls().to_plugin_value()),
        "getDistReference" => Ok(package.get_dist_reference().to_plugin_value()),
        "getDistSha1Checksum" => Ok(package.get_dist_sha1_checksum().to_plugin_value()),
        "getDistMirrors" => Ok(package.get_dist_mirrors().to_plugin_value()),
        "getVersion" => Ok(package.get_version().to_string().to_plugin_value()),
        "getPrettyVersion" => Ok(package.get_pretty_version().to_string().to_plugin_value()),
        "getFullPrettyVersion" => {
            let truncate = arg_or::<bool>(method_name, args, 0, true)?;
            let display_mode =
                arg_or::<DisplayMode>(method_name, args, 1, DisplayMode::SourceRefIfDev)?;
            Ok(package
                .get_full_pretty_version(truncate, display_mode)
                .to_plugin_value())
        }
        "getStability" => Ok(package.get_stability().to_string().to_plugin_value()),
        "getRequires" => Ok((*package.get_requires()).clone().to_plugin_value()),
        "getConflicts" => Ok((*package.get_conflicts()).clone().to_plugin_value()),
        "getProvides" => Ok((*package.get_provides()).clone().to_plugin_value()),
        "getReplaces" => Ok((*package.get_replaces()).clone().to_plugin_value()),
        "getDevRequires" => Ok((*package.get_dev_requires()).clone().to_plugin_value()),
        "getSuggests" => Ok(package.get_suggests().to_plugin_value()),
        "getAutoload" => Ok(package.get_autoload().to_plugin_value()),
        "getDevAutoload" => Ok(package.get_dev_autoload().to_plugin_value()),
        "getIncludePaths" => Ok(package.get_include_paths().to_plugin_value()),
        "getPhpExt" => Ok(match package.get_php_ext() {
            Some(config) => config.to_plugin_value(),
            None => PluginValue::Null,
        }),
        "getRepository" => match package.get_repository() {
            Some(repository) => repository_handle_value(&repository),
            None => Ok(PluginValue::Null),
        },
        "getBinaries" => Ok(package.get_binaries().to_plugin_value()),
        "getUniqueName" => Ok(package.get_unique_name().to_plugin_value()),
        "getNotificationUrl" => Ok(package.get_notification_url().to_plugin_value()),
        "__toString" => Ok(package.get_unique_name().to_plugin_value()),
        "getPrettyString" => Ok(package.get_pretty_string().to_plugin_value()),
        "isDefaultBranch" => Ok(package.is_default_branch().to_plugin_value()),
        // `BasePackage`'s concrete methods are not forwarded by `PackageInterface`, so both are
        // computed from the interface here, as `VersionSelector` already does for the second.
        "isPlatform" => Ok(package
            .get_repository()
            .is_some_and(|repository| repository.is::<crate::repository::PlatformRepository>())
            .to_plugin_value()),
        "getStabilityPriority" => Ok((*crate::package::base_package::STABILITIES
            .get(package.get_stability())
            .unwrap_or(&crate::package::base_package::STABILITY_STABLE))
        .to_plugin_value()),
        "getTransportOptions" => Ok(package.get_transport_options().to_plugin_value()),
        "getReleaseDate" => Ok(package.get_release_date().to_plugin_value()),
        other => Err(runtime_throw(format!(
            "the package method `{other}` is not available over RPC yet"
        ))),
    }
}

fn dispatch_operation_method(
    operation: &AnyOperation,
    method_name: &str,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match (method_name, operation) {
        ("getOperationType", _) => Ok(operation.get_operation_type().to_plugin_value()),
        ("show", _) => Ok(operation
            .show(arg::<bool>(method_name, args, 0)?)
            .to_plugin_value()),
        ("__toString", _) => Ok(operation.to_string().to_plugin_value()),
        ("getPackage", AnyOperation::Install(op)) => {
            Ok(package_handle_value(op.get_package().as_rc()))
        }
        ("getPackage", AnyOperation::Uninstall(op)) => {
            Ok(package_handle_value(op.get_package().as_rc()))
        }
        ("getPackage", AnyOperation::MarkAliasInstalled(op)) => {
            Ok(package_handle_value(op.get_package().as_rc()))
        }
        ("getPackage", AnyOperation::MarkAliasUninstalled(op)) => {
            Ok(package_handle_value(op.get_package().as_rc()))
        }
        ("getInitialPackage", AnyOperation::Update(op)) => {
            Ok(package_handle_value(op.get_initial_package().as_rc()))
        }
        ("getTargetPackage", AnyOperation::Update(op)) => {
            Ok(package_handle_value(op.get_target_package().as_rc()))
        }
        (other, _) => Err(runtime_throw(format!(
            "the operation method `{other}` is not available on a {} over RPC yet",
            operation_stub_class(operation)
        ))),
    }
}

fn dispatch_installation_manager_method(
    im: &std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>>,
    method_name: &str,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name {
        "getInstallPath" => {
            let package = arg::<PackageInterfaceHandle>(method_name, args, 0)?;
            Ok(im.borrow().get_install_path(package).to_plugin_value())
        }
        "addInstaller" | "removeInstaller" => {
            let handle = arg::<PhpObjHandle>(method_name, args, 0)?;
            if !php_is_a(&handle, "Composer\\Installer\\InstallerInterface").map_err(|error| {
                runtime_throw(format!(
                    "{method_name} could not type-check its argument: {error:#}"
                ))
            })? {
                return Err(runtime_throw(format!(
                    "{method_name} expects a Composer\\Installer\\InstallerInterface, got {}",
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
        other => Err(runtime_throw(format!(
            "the InstallationManager method `{other}` is not available over RPC yet"
        ))),
    }
}

fn dispatch_io_method(
    io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    method_name: &str,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name {
        "write" | "writeError" => {
            let (messages, newline, verbosity) = decode_write_args(method_name, args)?;
            let borrowed = io.borrow();
            for message in &messages {
                if method_name == "write" {
                    borrowed.write3(message, newline, verbosity);
                } else {
                    borrowed.write_error3(message, newline, verbosity);
                }
            }
            Ok(PluginValue::Null)
        }
        "isInteractive" => Ok(io.borrow().is_interactive().to_plugin_value()),
        "isVerbose" => Ok(io.borrow().is_verbose().to_plugin_value()),
        "isVeryVerbose" => Ok(io.borrow().is_very_verbose().to_plugin_value()),
        "isDebug" => Ok(io.borrow().is_debug().to_plugin_value()),
        "isDecorated" => Ok(io.borrow().is_decorated().to_plugin_value()),
        // TODO(plugin): the remaining IOInterface surface (ask*, authentications, ...) is
        // widened on demand, driven by explicit errors from real plugins.
        other => Err(runtime_throw(format!(
            "the IO method `{other}` is not available over RPC yet"
        ))),
    }
}

/// Decodes `($messages, bool $newline, int $verbosity)`: `$messages` is a string or a list of
/// strings in PHP.
fn decode_write_args(
    method_name: &str,
    args: &[PluginValue],
) -> Result<(Vec<String>, bool, i64), PhpThrow> {
    let messages = match args.first() {
        Some(PluginValue::String(_)) => vec![arg::<String>(method_name, args, 0)?],
        _ => arg::<Vec<String>>(method_name, args, 0)?,
    };
    let newline = arg_or::<bool>(method_name, args, 1, true)?;
    let verbosity = arg_or::<i64>(method_name, args, 2, crate::io::NORMAL)?;
    Ok((messages, newline, verbosity))
}

fn runtime_throw(message: String) -> PhpThrow {
    PhpThrow {
        exception_class: "RuntimeException".to_string(),
        message,
        code: 0,
    }
}

/// `PluginInterface` adapter for a plugin entity living in the PHP child process: every
/// lifecycle call is forwarded as a `CallPhpMethod` RPC.
#[derive(Debug)]
pub struct PhpPluginProxy {
    pub(crate) phandle: u64,
    pub(crate) class: String,
    /// Every interface the entity's class implements (`class_implements` in the child), for
    /// the `instanceof` checks PHP performs on plugin objects.
    pub(crate) implements: Vec<String>,
}

impl PhpPluginProxy {
    pub fn new(phandle: u64, class: String, implements: Vec<String>) -> Self {
        Self {
            phandle,
            class,
            implements,
        }
    }

    fn forward_lifecycle_call(
        &self,
        method: &str,
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
            // TODO(plugin): the original exception class is collapsed to RuntimeException on
            // this side of the boundary.
            Err(throw) => {
                Err(shirabe_php_shim::RuntimeException::with_code(throw.message, throw.code).into())
            }
        }
    }

    /// For testing only: reads a public property of the plugin entity in the child, mirroring
    /// PHPUnit assertions like `$plugins[0]->version`.
    pub fn __get_property(&self, name: &str) -> anyhow::Result<shirabe_php_shim::PhpMixed> {
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
            Err(throw) => {
                Err(shirabe_php_shim::RuntimeException::with_code(throw.message, throw.code).into())
            }
        }
    }
}

impl PluginInterface for PhpPluginProxy {
    fn activate(
        &mut self,
        composer: ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        self.forward_lifecycle_call("activate", &composer, &io)
    }

    fn deactivate(
        &mut self,
        composer: ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        self.forward_lifecycle_call("deactivate", &composer, &io)
    }

    fn uninstall(
        &mut self,
        composer: ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()> {
        self.forward_lifecycle_call("uninstall", &composer, &io)
    }

    fn get_class_name(&self) -> String {
        self.class.clone()
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
    fn get_subscribed_events(&self) -> anyhow::Result<IndexMap<String, SubscribedEventEntry>> {
        // PHP: `$subscriber->getSubscribedEvents()` (an instance call of the static method).
        let outcome = call_php_method(
            self.phandle,
            "getSubscribedEvents",
            Vec::new(),
            Some(&mut PluginRpcDispatcher::default()),
        )?;
        let value = match outcome {
            Ok(value) => value,
            // TODO(plugin): the original exception class is collapsed to RuntimeException on
            // this side of the boundary.
            Err(throw) => {
                return Err(shirabe_php_shim::RuntimeException::with_code(
                    throw.message,
                    throw.code,
                )
                .into());
            }
        };
        decode_subscribed_events(&self.class, value)
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
    fn get_capabilities(&self) -> anyhow::Result<IndexMap<String, PhpMixed>> {
        let outcome = call_php_method(
            self.phandle,
            "getCapabilities",
            Vec::new(),
            Some(&mut PluginRpcDispatcher::default()),
        )?;
        let value = match outcome {
            Ok(value) => value,
            // TODO(plugin): the original exception class is collapsed to RuntimeException on
            // this side of the boundary.
            Err(throw) => {
                return Err(shirabe_php_shim::RuntimeException::with_code(
                    throw.message,
                    throw.code,
                )
                .into());
            }
        };
        // PHP: `(array) $plugin->getCapabilities()` — the interface declares no return type,
        // so a non-array return is cast. A handle in the map (a plugin putting an object among
        // its capability values) has no PhpMixed image and fails the conversion explicitly.
        let entries = match value {
            PluginValue::Null => IndexMap::new(),
            PluginValue::Array(map) | PluginValue::Object(map) => map
                .into_iter()
                .map(|(k, v)| Ok((String::from_utf8_lossy(&k).into_owned(), v.to_php_mixed()?)))
                .collect::<anyhow::Result<_>>()?,
            PluginValue::List(items) => items
                .into_iter()
                .enumerate()
                .map(|(i, v)| Ok((i.to_string(), v.to_php_mixed()?)))
                .collect::<anyhow::Result<_>>()?,
            scalar => IndexMap::from([("0".to_string(), scalar.to_php_mixed()?)]),
        };
        Ok(entries)
    }
}

/// Decodes a `getSubscribedEvents()` wire value into the three shapes `addSubscriber`
/// distinguishes: `'method'`, `['method', priority]`, and `[['method', priority], ...]`.
/// A shape outside these is an explicit error, never a silently dropped listener.
fn decode_subscribed_events(
    class: &str,
    value: PluginValue,
) -> anyhow::Result<IndexMap<String, SubscribedEventEntry>> {
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
    let mut events: IndexMap<String, SubscribedEventEntry> = IndexMap::new();
    for (event_name, params) in entries {
        // TODO(bytes): lossy UTF-8; event and method names are bytes in PHP.
        let event_name = String::from_utf8_lossy(&event_name).into_owned();
        let entry = match &params {
            PluginValue::String(method) => {
                SubscribedEventEntry::Method(String::from_utf8_lossy(method).into_owned())
            }
            PluginValue::List(items) => match items.first() {
                Some(PluginValue::String(method)) => SubscribedEventEntry::MethodWithPriority(
                    String::from_utf8_lossy(method).into_owned(),
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
                            String::from_utf8_lossy(method).into_owned(),
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
    class: &str,
    value: Option<&PluginValue>,
) -> anyhow::Result<Option<i64>> {
    match value {
        None => Ok(None),
        Some(PluginValue::Int(priority)) => Ok(Some(*priority)),
        Some(other) => Err(subscribed_events_shape_error(class, other)),
    }
}

fn subscribed_events_shape_error(class: &str, value: &PluginValue) -> anyhow::Error {
    shirabe_php_shim::RuntimeException::new(format!(
        "{class}::getSubscribedEvents() returned an unsupported shape over RPC: {value:?}"
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
pub(crate) fn php_class_query(function: &str, args: Vec<PluginValue>) -> anyhow::Result<bool> {
    crate::event_dispatcher::EventDispatcher::ensure_script_autoloader()?;
    let value = unwrap_php_result(call_function_with_dispatcher(
        function,
        args,
        Some(&mut PluginRpcDispatcher::default()),
    ))?;
    match value {
        PluginValue::Bool(value) => Ok(value),
        other => Err(anyhow::anyhow!(
            "PHP class query `{function}` did not return a bool: {other:?}"
        )),
    }
}

/// Instantiates `new $class(...$ctor_args)` in the worker, with the script autoloader active.
pub(crate) fn new_php_object(
    class: &str,
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
        other => Err(shirabe_php_shim::RuntimeException::new(format!(
            "`new {class}` returned an unsupported shape over RPC: {other:?}"
        ))
        .into()),
    }
}

/// Calls `$obj->$method(...$args)` on a worker-side entity.
pub(crate) fn call_php_entity_method(
    handle: &PhpObjHandle,
    method: &str,
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
pub(crate) fn php_is_a(handle: &PhpObjHandle, class: &str) -> anyhow::Result<bool> {
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

    fn call(&self, method: &str, args: Vec<PluginValue>) -> anyhow::Result<PluginValue> {
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
    /// installer's effects rather than chaining continuations, so a returned promise is drained
    /// here: an already-settled one yields its value (or raises its rejection reason), while a
    /// still-pending one is an explicit error.
    ///
    /// TODO(async): resolving a still-pending promise would need a concurrent execution engine
    /// the boundary does not have.
    fn promise_result(&self, method: &str, value: PluginValue) -> anyhow::Result<Option<PhpMixed>> {
        let handle = match value {
            PluginValue::Null => return Ok(None),
            PluginValue::PhpHandle(handle) => handle,
            other => return Err(self.unsupported_shape(method, &other)),
        };
        if !php_is_a(&handle, "React\\Promise\\PromiseInterface")? {
            return Err(self.unsupported_shape(method, &PluginValue::PhpHandle(handle)));
        }
        let phandle = handle.phandle;
        let settled = unwrap_php_result(call_function_with_dispatcher(
            "__shirabe_settle_promise",
            vec![PluginValue::PhpHandle(handle)],
            Some(&mut PluginRpcDispatcher::default()),
        ));
        // The promise entity was interned in the worker's P table when it crossed; it has no
        // owner on this side beyond this call, on the rejection path too.
        let _ = release_php_handle(phandle);
        match settled? {
            PluginValue::Null => Ok(None),
            other => Ok(Some(other.to_php_mixed()?)),
        }
    }

    fn unsupported_shape(&self, method: &str, value: &PluginValue) -> anyhow::Error {
        shirabe_php_shim::RuntimeException::new(format!(
            "{}::{method}() returned an unsupported shape over RPC: {value:?}",
            self.handle.class
        ))
        .into()
    }
}

#[async_trait::async_trait(?Send)]
impl crate::installer::InstallerInterface for PhpInstallerProxy {
    fn supports(&self, package_type: &str) -> anyhow::Result<bool> {
        match self.call("supports", vec![PluginValue::string(package_type)])? {
            PluginValue::Bool(supports) => Ok(supports),
            other => Err(self.unsupported_shape("supports", &other)),
        }
    }

    fn is_installed(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<bool> {
        let args = vec![Self::repo_arg(repo)?, Self::package_arg(&package)];
        match self.call("isInstalled", args)? {
            PluginValue::Bool(installed) => Ok(installed),
            other => Err(self.unsupported_shape("isInstalled", &other)),
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
        let value = self.call("download", args)?;
        self.promise_result("download", value)
    }

    async fn prepare(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![
            PluginValue::string(r#type),
            Self::package_arg(&package),
            Self::optional_package_arg(&prev_package),
        ];
        let value = self.call("prepare", args)?;
        self.promise_result("prepare", value)
    }

    async fn install(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![Self::repo_arg(repo)?, Self::package_arg(&package)];
        let value = self.call("install", args)?;
        self.promise_result("install", value)
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
        let value = self.call("update", args)?;
        self.promise_result("update", value)
    }

    async fn uninstall(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![Self::repo_arg(repo)?, Self::package_arg(&package)];
        let value = self.call("uninstall", args)?;
        self.promise_result("uninstall", value)
    }

    async fn cleanup(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![
            PluginValue::string(r#type),
            Self::package_arg(&package),
            Self::optional_package_arg(&prev_package),
        ];
        let value = self.call("cleanup", args)?;
        self.promise_result("cleanup", value)
    }

    fn get_install_path(&self, package: PackageInterfaceHandle) -> Option<String> {
        // PHP declares `getInstallPath(): string`; a failure here is a plugin error the
        // infallible signature cannot carry, so it aborts rather than answering a path that
        // would silently install the package in the wrong place.
        let args = vec![Self::package_arg(&package)];
        let value = self.call("getInstallPath", args).unwrap_or_else(|error| {
            panic!(
                "{}::getInstallPath failed over RPC: {error:#}",
                self.handle.class
            )
        });
        match value {
            PluginValue::Null => None,
            PluginValue::String(path) => Some(String::from_utf8_lossy(&path).into_owned()),
            other => panic!("{}", self.unsupported_shape("getInstallPath", &other)),
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
    pub fn __get_property(&self, name: &str) -> anyhow::Result<PluginValue> {
        let outcome = shirabe_php_rpc::call_function(
            "__shirabe_get_property",
            vec![
                PluginValue::PhpHandle(self.handle.clone()),
                PluginValue::string(name),
            ],
        )?;
        match outcome {
            Ok(value) => Ok(value),
            Err(throw) => {
                Err(shirabe_php_shim::RuntimeException::with_code(throw.message, throw.code).into())
            }
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
                return Err(shirabe_php_shim::UnexpectedValueException::new(format!(
                    "Plugin capability {} failed to return an array from getCommands",
                    self.handle.class
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
            if !php_is_a(&command_handle, "Composer\\Command\\BaseCommand")? {
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
    shirabe_php_shim::UnexpectedValueException::new(format!(
        "Plugin capability {} returned an invalid value, we expected an array of Composer\\Command\\BaseCommand objects",
        capability.class
    )).into()
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
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) aliases: Vec<String>,
    pub(crate) hidden: bool,
}

impl RustCommandMetadata {
    fn wire_value(&self) -> PluginValue {
        let mut row: IndexMap<Vec<u8>, PluginValue> = IndexMap::new();
        row.insert(b"name".to_vec(), PluginValue::string(self.name.clone()));
        row.insert(
            b"description".to_vec(),
            PluginValue::string(self.description.clone()),
        );
        row.insert(
            b"aliases".to_vec(),
            PluginValue::List(
                self.aliases
                    .iter()
                    .map(|alias| PluginValue::string(alias.clone()))
                    .collect(),
            ),
        );
        row.insert(b"hidden".to_vec(), PluginValue::Bool(self.hidden));
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
    initial_working_directory: Option<String>,
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
    initial_working_directory: Option<String>,
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
            call_php_entity_method(app, "add", vec![PluginValue::PhpHandle(handle.clone())])?;
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
        let mut config: IndexMap<Vec<u8>, PluginValue> = IndexMap::new();
        config.insert(
            b"composer".to_vec(),
            match &self.composer {
                Some(composer) => composer_handle_value(composer),
                None => PluginValue::Null,
            },
        );
        config.insert(b"io".to_vec(), io_handle_value(&self.io)?);
        config.insert(
            b"initialWorkingDirectory".to_vec(),
            match &self.initial_working_directory {
                Some(dir) => PluginValue::string(dir.clone()),
                None => PluginValue::Null,
            },
        );
        config.insert(
            b"disablePluginsByDefault".to_vec(),
            PluginValue::Bool(self.disable_plugins_by_default),
        );
        config.insert(
            b"disableScriptsByDefault".to_vec(),
            PluginValue::Bool(self.disable_scripts_by_default),
        );
        config.insert(
            b"rustCommands".to_vec(),
            PluginValue::List(
                self.rust_commands
                    .iter()
                    .map(RustCommandMetadata::wire_value)
                    .collect(),
            ),
        );
        config.insert(
            b"pluginCommands".to_vec(),
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
                return Err(shirabe_php_shim::RuntimeException::new(
                    format!(
                        "__shirabe_console_application_boot returned an unsupported shape over RPC: {other:?}"
                    )
                ).into());
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
        let proxy_command = match Self::call_metadata_getter(&handle, "isProxyCommand")? {
            PluginValue::Bool(proxy_command) => proxy_command,
            other => return Err(Self::unsupported_shape(&handle, "isProxyCommand", &other)),
        };
        Self::build(handle, proxy_command)
    }

    /// A command class named by a `composer.json` script only has to extend Symfony's `Command`,
    /// so `isProxyCommand()` is asked for only when it also extends Composer's `BaseCommand`.
    pub(crate) fn new_script_command(handle: PhpObjHandle) -> anyhow::Result<Self> {
        let proxy_command = if php_is_a(&handle, "Composer\\Command\\BaseCommand")? {
            match Self::call_metadata_getter(&handle, "isProxyCommand")? {
                PluginValue::Bool(proxy_command) => proxy_command,
                other => return Err(Self::unsupported_shape(&handle, "isProxyCommand", &other)),
            }
        } else {
            false
        };
        Self::build(handle, proxy_command)
    }

    fn build(handle: PhpObjHandle, proxy_command: bool) -> anyhow::Result<Self> {
        let data = crate::command::BaseCommandData::new(None);
        let name = Self::call_metadata_getter(&handle, "getName")?;
        match name {
            PluginValue::Null => {}
            PluginValue::String(bytes) => {
                Command::set_name(&data, &String::from_utf8_lossy(&bytes))?;
            }
            other => return Err(Self::unsupported_shape(&handle, "getName", &other)),
        }
        let description = Self::call_metadata_getter(&handle, "getDescription")?;
        match description {
            PluginValue::String(bytes) => {
                Command::set_description(&data, &String::from_utf8_lossy(&bytes));
            }
            other => return Err(Self::unsupported_shape(&handle, "getDescription", &other)),
        }
        let aliases = Self::call_metadata_getter(&handle, "getAliases")?;
        let alias_items = match aliases {
            PluginValue::List(items) => items,
            PluginValue::Array(map) => map.into_values().collect(),
            other => return Err(Self::unsupported_shape(&handle, "getAliases", &other)),
        };
        let mut alias_names = Vec::new();
        for alias in alias_items {
            match alias {
                PluginValue::String(bytes) => {
                    alias_names.push(String::from_utf8_lossy(&bytes).into_owned());
                }
                other => return Err(Self::unsupported_shape(&handle, "getAliases", &other)),
            }
        }
        Command::set_aliases(&data, alias_names)?;
        let hidden = Self::call_metadata_getter(&handle, "isHidden")?;
        match hidden {
            PluginValue::Bool(hidden) => {
                Command::set_hidden(&data, hidden);
            }
            other => return Err(Self::unsupported_shape(&handle, "isHidden", &other)),
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
        use shirabe_symfony_console::input::InputArgument;
        use shirabe_symfony_console::input::InputOption;
        use shirabe_symfony_console::input::{DefinitionItem, InputDefinition};

        let value = unwrap_php_result(call_function_with_dispatcher(
            "__shirabe_read_command_definition",
            vec![PluginValue::PhpHandle(handle.clone())],
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        let mut map = match value {
            PluginValue::Array(map) => map,
            other => return Err(Self::unsupported_shape(handle, "getDefinition", &other)),
        };
        let field = |row: &mut IndexMap<Vec<u8>, PluginValue>, key: &str| -> PluginValue {
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
        for row in as_rows(field(&mut map, "arguments")) {
            let mut row = match row {
                PluginValue::Array(row) => row,
                other => return Err(Self::unsupported_shape(handle, "getDefinition", &other)),
            };
            let (name, description) =
                match (field(&mut row, "name"), field(&mut row, "description")) {
                    (PluginValue::String(name), PluginValue::String(description)) => (
                        String::from_utf8_lossy(&name).into_owned(),
                        String::from_utf8_lossy(&description).into_owned(),
                    ),
                    (other, _) => {
                        return Err(Self::unsupported_shape(handle, "getDefinition", &other));
                    }
                };
            let required = matches!(field(&mut row, "required"), PluginValue::Bool(true));
            let is_array = matches!(field(&mut row, "isArray"), PluginValue::Bool(true));
            let mut mode = if required {
                InputArgument::REQUIRED
            } else {
                InputArgument::OPTIONAL
            };
            if is_array {
                mode |= InputArgument::IS_ARRAY;
            }
            let default = InputValue::from_php_mixed(&field(&mut row, "default").to_php_mixed()?);
            items.push(DefinitionItem::InputArgument(InputArgument::new(
                name,
                Some(mode),
                description,
                default,
            )?));
        }
        for row in as_rows(field(&mut map, "options")) {
            let mut row = match row {
                PluginValue::Array(row) => row,
                other => return Err(Self::unsupported_shape(handle, "getDefinition", &other)),
            };
            let (name, description) =
                match (field(&mut row, "name"), field(&mut row, "description")) {
                    (PluginValue::String(name), PluginValue::String(description)) => (
                        String::from_utf8_lossy(&name).into_owned(),
                        String::from_utf8_lossy(&description).into_owned(),
                    ),
                    (other, _) => {
                        return Err(Self::unsupported_shape(handle, "getDefinition", &other));
                    }
                };
            let accept_value = matches!(field(&mut row, "acceptValue"), PluginValue::Bool(true));
            let mut mode = if accept_value {
                if matches!(field(&mut row, "isValueRequired"), PluginValue::Bool(true)) {
                    InputOption::VALUE_REQUIRED
                } else {
                    InputOption::VALUE_OPTIONAL
                }
            } else {
                InputOption::VALUE_NONE
            };
            if matches!(field(&mut row, "isArray"), PluginValue::Bool(true)) {
                mode |= InputOption::VALUE_IS_ARRAY;
            }
            if matches!(field(&mut row, "isNegatable"), PluginValue::Bool(true)) {
                mode |= InputOption::VALUE_NEGATABLE;
            }
            let shortcut = match field(&mut row, "shortcut").to_php_mixed()? {
                PhpMixed::String(shortcut) => Some(shortcut),
                _ => None,
            };
            // `getDefault()` exposes the stored representation (`false` for VALUE_NONE), while
            // the constructor only accepts null there; mirror the constructor's normalization.
            let default = if accept_value {
                InputValue::from_php_mixed(&field(&mut row, "default").to_php_mixed()?)
            } else {
                InputValue::Null
            };
            items.push(DefinitionItem::InputOption(InputOption::new(
                &name,
                shortcut.as_deref(),
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

        match field(&mut map, "help") {
            PluginValue::String(help) => {
                Command::set_help(data, &String::from_utf8_lossy(&help));
            }
            other => return Err(Self::unsupported_shape(handle, "getHelp", &other)),
        }
        for usage in as_rows(field(&mut map, "usages")) {
            match usage {
                PluginValue::String(usage) => {
                    Command::add_usage(data, &String::from_utf8_lossy(&usage));
                }
                other => return Err(Self::unsupported_shape(handle, "getUsages", &other)),
            }
        }
        Ok(())
    }

    fn call_metadata_getter(handle: &PhpObjHandle, method: &str) -> anyhow::Result<PluginValue> {
        unwrap_php_result(call_php_method(
            handle.phandle,
            method,
            Vec::new(),
            Some(&mut PluginRpcDispatcher::default()),
        ))
    }

    fn unsupported_shape(
        handle: &PhpObjHandle,
        method: &str,
        value: &PluginValue,
    ) -> anyhow::Error {
        shirabe_php_shim::RuntimeException::new(format!(
            "{}::{method}() returned an unsupported shape over RPC: {value:?}",
            handle.class
        ))
        .into()
    }
}

impl Command for PhpCommandProxy {
    /// Forwards the whole run to the worker-side console application (the proxy-command idiom
    /// the trait allows): the real Symfony machinery there performs input binding, validation,
    /// interaction and execution against the live PHP command object, writing to the stdio the
    /// worker inherited. The Rust-side `base_run` half must not run against the mirrored
    /// definition, or binding and interaction would happen twice.
    fn run(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let context = CONSOLE_APP_CONTEXT
            .with(|slot| slot.borrow().clone())
            .ok_or_else(|| -> anyhow::Error {
                shirabe_php_shim::RuntimeException::new(format!(
                        "cannot run plugin-provided command {}: no worker-side console application context was published",
                        self.handle.class
                    )).into()
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
            other => Err(Self::unsupported_shape(&self.handle, "run", &other)),
        }
    }

    fn execute(
        &self,
        _input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        // `run` above never reaches this template hook; a direct call would bypass the
        // worker-side binding, so it stays an explicit error.
        Err(shirabe_php_shim::RuntimeException::new(
            format!(
                "plugin-provided command {} executes in the PHP worker through run(); execute() must not be called directly",
                self.handle.class
            )
        ).into())
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

impl shirabe_php_shim::PhpClass for PhpCommandProxy {
    fn php_class_name(&self) -> String {
        self.handle.class.clone()
    }
}

impl Drop for PhpCommandProxy {
    fn drop(&mut self) {
        let _ = release_php_handle(self.handle.phandle);
    }
}
