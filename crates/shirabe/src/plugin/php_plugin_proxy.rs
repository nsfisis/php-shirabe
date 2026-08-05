//! PHP-backed `PluginInterface` adapter and the R table serving its RPC callbacks.
//!
//! This module has no Composer counterpart: it is the Rust half of the plugin runtime split
//! (a real PHP child process executes the plugin code, see `docs/dev/php-rpc.md`). The plugin
//! entity lives in the worker's P table; Rust-side entities the plugin can call back into
//! (`$composer`, `$io`) live in the R table here.

use crate::autoload::ClassLoader;
use crate::command::BaseCommand;
use crate::composer::ComposerHandle;
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
use crate::plugin::plugin_interface::PluginInterface;
use crate::repository::{
    InstalledArrayRepository, InstalledFilesystemRepository, InstalledRepositoryInterfaceHandle,
    RepositoryInterfaceHandle, RepositoryManagerInterface,
};
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_rpc::{
    PhpObjHandle, PhpThrow, PluginValue, RustMethodDispatcher, RustObjHandle,
    call_function_with_dispatcher, call_php_method, release_php_handle,
};
use shirabe_php_shim::PhpMixed;

/// A Rust-side entity a PHP proxy stub points back to.
#[derive(Debug, Clone)]
enum RustEntity {
    Composer(ComposerHandle),
    Io(std::rc::Rc<std::cell::RefCell<dyn IOInterface>>),
    InstallationManager(std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>>),
    RepositoryManager(std::rc::Rc<std::cell::RefCell<dyn RepositoryManagerInterface>>),
    Repository(RepositoryInterfaceHandle),
    Package(std::rc::Rc<std::cell::RefCell<AnyPackage>>),
    EventDispatcher(
        std::rc::Rc<std::cell::RefCell<dyn crate::event_dispatcher::EventDispatcherInterface>>,
    ),
}

/// The pointer identity backing R-table interning: the same shared instance must always cross
/// the boundary as the same handle (`===` in the child).
fn entity_ptr_id(entity: &RustEntity) -> usize {
    match entity {
        RustEntity::Composer(composer) => {
            std::rc::Rc::as_ptr(composer.as_rc()) as *const () as usize
        }
        RustEntity::Io(io) => std::rc::Rc::as_ptr(io) as *const () as usize,
        RustEntity::InstallationManager(im) => std::rc::Rc::as_ptr(im) as *const () as usize,
        RustEntity::RepositoryManager(rm) => std::rc::Rc::as_ptr(rm) as *const () as usize,
        RustEntity::Repository(repository) => repository.ptr_id(),
        RustEntity::Package(package) => std::rc::Rc::as_ptr(package) as *const () as usize,
        RustEntity::EventDispatcher(dispatcher) => {
            std::rc::Rc::as_ptr(dispatcher) as *const () as usize
        }
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
fn package_stub_class(
    package: &std::rc::Rc<std::cell::RefCell<AnyPackage>>,
) -> Result<&'static str, PhpThrow> {
    match &*package.borrow() {
        AnyPackage::Package(_) => Ok("Composer\\Package\\Package"),
        AnyPackage::CompletePackage(_) => Ok("Composer\\Package\\CompletePackage"),
        AnyPackage::RootPackage(_) => Ok("Composer\\Package\\RootPackage"),
        // TODO(plugin): alias packages need proxy stubs of their own before they can cross.
        AnyPackage::AliasPackage(_)
        | AnyPackage::CompleteAliasPackage(_)
        | AnyPackage::RootAliasPackage(_) => Err(runtime_throw(
            "alias packages are not available over RPC yet".to_string(),
        )),
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

/// Registers a package and returns its wire descriptor.
pub(crate) fn package_handle_value(
    package: &std::rc::Rc<std::cell::RefCell<AnyPackage>>,
) -> Result<PluginValue, PhpThrow> {
    let class = package_stub_class(package)?;
    let rhandle = register_entity(RustEntity::Package(package.clone()));
    Ok(rust_handle_value(rhandle, class))
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
                let class = match args.first() {
                    // TODO(phase-e): lossy UTF-8; class names are bytes in PHP.
                    Some(PluginValue::String(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
                    other => {
                        return Err(runtime_throw(format!(
                            "__shirabe_find_file expects a class name argument, got {other:?}"
                        )));
                    }
                };
                return Ok(match find_file_in_registered_loaders(&class) {
                    Some(file) => PluginValue::string(file),
                    None => PluginValue::Null,
                });
            }
            if method_name == "__shirabe_run_rust_command" {
                let (name, input_line) = match (args.first(), args.get(1)) {
                    (Some(PluginValue::String(name)), Some(PluginValue::String(line))) => (
                        // TODO(phase-e): lossy UTF-8; command lines are bytes in PHP.
                        String::from_utf8_lossy(name).into_owned(),
                        String::from_utf8_lossy(line).into_owned(),
                    ),
                    _ => {
                        return Err(runtime_throw(format!(
                            "__shirabe_run_rust_command expects a command name and an input line, got {args:?}"
                        )));
                    }
                };
                return match crate::console::application::run_worker_reverse_command(
                    &name,
                    &input_line,
                ) {
                    Ok(code) => Ok(PluginValue::Int(code)),
                    Err(e) => Err(runtime_throw(format!("{e:#}"))),
                };
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

        // The entity is cloned out so no table borrow is held while the handler runs (a
        // handler that re-enters register_*_entity would otherwise panic on the RefCell).
        let entity = R_TABLE.with(|table| table.borrow().get(&rhandle).cloned());
        if method_name == "__shirabeClone" {
            return match entity {
                Some(entity) => clone_entity(&entity),
                None => Err(runtime_throw(format!("unknown Rust handle {rhandle}"))),
            };
        }
        match entity {
            Some(RustEntity::Io(io)) => dispatch_io_method(&io, method_name, &args),
            Some(RustEntity::Composer(composer)) => {
                dispatch_composer_method(&composer, method_name)
            }
            Some(RustEntity::InstallationManager(im)) => {
                dispatch_installation_manager_method(&im, method_name, &args)
            }
            Some(RustEntity::RepositoryManager(rm)) => {
                dispatch_repository_manager_method(&rm, method_name)
            }
            Some(RustEntity::Repository(repository)) => {
                dispatch_repository_method(&repository, method_name)
            }
            Some(RustEntity::Package(package)) => {
                dispatch_package_method(&package, method_name, &args)
            }
            Some(RustEntity::EventDispatcher(dispatcher)) => {
                dispatch_event_dispatcher_method(&dispatcher, method_name, &args)
            }
            None => Err(runtime_throw(format!("unknown Rust handle {rhandle}"))),
        }
    }
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
        | RustEntity::Io(_)
        | RustEntity::InstallationManager(_)
        | RustEntity::RepositoryManager(_)
        | RustEntity::Repository(_)
        | RustEntity::EventDispatcher(_) => {
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
            package_handle_value(&package)
        }
        "getEventDispatcher" => {
            let dispatcher = composer.borrow().get_event_dispatcher();
            let rhandle = register_entity(RustEntity::EventDispatcher(dispatcher));
            Ok(rust_handle_value(
                rhandle,
                "Composer\\EventDispatcher\\EventDispatcher",
            ))
        }
        // TODO(plugin): the remaining Composer object graph (getConfig, getLocker, ...)
        // becomes reachable over RPC on demand, driven by explicit errors from real plugins.
        other => Err(runtime_throw(format!(
            "the Composer method `{other}` is not available over RPC yet"
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
            let name = match args.first() {
                Some(PluginValue::String(name)) => String::from_utf8_lossy(name).into_owned(),
                other => {
                    return Err(runtime_throw(format!(
                        "dispatch expects an event name, got {other:?}"
                    )));
                }
            };
            let probe = crate::event_dispatcher::Event::from_name(name.clone());
            if dispatcher.borrow_mut().has_event_listeners(&probe) {
                // TODO(plugin): dispatching a worker-constructed event through the Rust-side
                // dispatcher needs the event object (and the console input it carries) proxied
                // back into this process; until then only the no-listener case — where
                // upstream's dispatch is observably a no-op returning 0 — is supported.
                return Err(runtime_throw(format!(
                    "dispatching `{name}` from the plugin process is not supported yet while listeners are registered for it"
                )));
            }
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
        "getLocalRepository" => {
            let local = rm.borrow().get_local_repository();
            let class = repository_stub_class(&local)?;
            let rhandle = register_entity(RustEntity::Repository(local));
            Ok(rust_handle_value(rhandle, class))
        }
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
) -> Result<PluginValue, PhpThrow> {
    match method_name {
        "getPackages" => {
            let packages = repository.borrow_mut().get_packages().map_err(|error| {
                // TODO(plugin): the original exception class is collapsed to RuntimeException
                // on this side of the boundary.
                runtime_throw(format!("getPackages failed over RPC: {error}"))
            })?;
            let mut items = Vec::with_capacity(packages.len());
            for package in packages {
                items.push(package_handle_value(package.as_rc())?);
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

/// A PHP string-or-null wire value.
fn optional_string(value: Option<String>) -> PluginValue {
    match value {
        Some(value) => PluginValue::string(value),
        None => PluginValue::Null,
    }
}

fn string_list(values: Vec<String>) -> PluginValue {
    PluginValue::List(values.into_iter().map(PluginValue::string).collect())
}

/// `?list<array{url: string, preferred: bool}>` as PHP shapes it.
fn mirror_list(mirrors: Option<Vec<crate::package::Mirror>>) -> PluginValue {
    match mirrors {
        None => PluginValue::Null,
        Some(mirrors) => PluginValue::List(
            mirrors
                .into_iter()
                .map(|mirror| {
                    PluginValue::Array(IndexMap::from([
                        (b"url".to_vec(), PluginValue::string(mirror.url)),
                        (b"preferred".to_vec(), PluginValue::Bool(mirror.preferred)),
                    ]))
                })
                .collect(),
        ),
    }
}

/// An `array<string, mixed>` as PHP shapes it: empty maps cross as a list, since an empty PHP
/// array is indistinguishable from an empty list on the wire.
fn string_keyed_map(map: IndexMap<String, PhpMixed>) -> PluginValue {
    if map.is_empty() {
        PluginValue::List(Vec::new())
    } else {
        PluginValue::from_php_mixed(&PhpMixed::Array(map))
    }
}

/// The inverse of `mirror_list`.
fn decode_mirrors(
    method: &str,
    value: Option<&PluginValue>,
) -> Result<Option<Vec<crate::package::Mirror>>, PhpThrow> {
    let rows = match value {
        None | Some(PluginValue::Null) => return Ok(None),
        Some(PluginValue::List(rows)) => rows.clone(),
        Some(PluginValue::Array(rows)) => rows.values().cloned().collect(),
        other => {
            return Err(runtime_throw(format!(
                "{method} expects a list of mirrors or null, got {other:?}"
            )));
        }
    };
    let mut mirrors = Vec::with_capacity(rows.len());
    for row in rows {
        let row = match row {
            PluginValue::Array(row) => row,
            other => {
                return Err(runtime_throw(format!(
                    "{method} expects mirror maps, got {other:?}"
                )));
            }
        };
        let url = match row.get(b"url".as_slice()) {
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
        mirrors.push(crate::package::Mirror { url, preferred });
    }
    Ok(Some(mirrors))
}

fn decode_optional_string(
    method: &str,
    value: Option<&PluginValue>,
) -> Result<Option<String>, PhpThrow> {
    match value {
        None | Some(PluginValue::Null) => Ok(None),
        Some(PluginValue::String(bytes)) => Ok(Some(String::from_utf8_lossy(bytes).into_owned())),
        other => Err(runtime_throw(format!(
            "{method} expects a string or null, got {other:?}"
        ))),
    }
}

fn dispatch_package_method(
    package: &std::rc::Rc<std::cell::RefCell<AnyPackage>>,
    method_name: &str,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    // The link getters return `array<string, Link>`; only the empty case has a wire image so
    // far (an empty PHP array crosses as a list).
    //
    // TODO(plugin): Link is a rust-snapshot value whose constraint field must materialize as a
    // real composer/semver object in the child; the snapshot encoding does not exist yet.
    let links = |links: IndexMap<String, crate::package::Link>| -> Result<PluginValue, PhpThrow> {
        if links.is_empty() {
            Ok(PluginValue::List(Vec::new()))
        } else {
            Err(runtime_throw(format!(
                "the package method `{method_name}` returns Link values, whose encoding over RPC is not implemented yet"
            )))
        }
    };

    // Mutators borrow mutably and must not hold the borrow across the shared-borrow arms.
    match method_name {
        "setId" => {
            let id = match args.first() {
                Some(PluginValue::Int(id)) => *id,
                other => {
                    return Err(runtime_throw(format!(
                        "setId expects an int, got {other:?}"
                    )));
                }
            };
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
            let value = decode_optional_string(method_name, args.first())?;
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
            let mirrors = decode_mirrors(method_name, args.first())?;
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
            let repository = match args.first() {
                Some(PluginValue::RustHandle(handle)) => {
                    match R_TABLE.with(|table| table.borrow().get(&handle.rhandle).cloned()) {
                        Some(RustEntity::Repository(repository)) => repository,
                        _ => {
                            return Err(runtime_throw(format!(
                                "setRepository expects a repository handle, got Rust handle {}",
                                handle.rhandle
                            )));
                        }
                    }
                }
                other => {
                    return Err(runtime_throw(format!(
                        "setRepository expects a repository argument, got {other:?}"
                    )));
                }
            };
            package
                .borrow_mut()
                .as_package_interface_mut()
                .set_repository(repository)
                .map_err(|error| runtime_throw(format!("setRepository failed: {error}")))?;
            return Ok(PluginValue::Null);
        }
        "setTransportOptions" => {
            let options = match args.first() {
                Some(value) => match value.to_php_mixed().map_err(|error| {
                    runtime_throw(format!(
                        "setTransportOptions could not decode its argument: {error:#}"
                    ))
                })? {
                    PhpMixed::Array(options) => options,
                    PhpMixed::List(items) if items.is_empty() => IndexMap::new(),
                    other => {
                        return Err(runtime_throw(format!(
                            "setTransportOptions expects an array, got {other:?}"
                        )));
                    }
                },
                None => IndexMap::new(),
            };
            package
                .borrow_mut()
                .as_package_interface_mut()
                .set_transport_options(options);
            return Ok(PluginValue::Null);
        }
        _ => {}
    }

    let borrowed = package.borrow();
    let package = borrowed.as_package_interface();
    match method_name {
        "getName" => Ok(PluginValue::string(package.get_name().to_string())),
        "getPrettyName" => Ok(PluginValue::string(package.get_pretty_name().to_string())),
        "getNames" => {
            let provides = match args.first() {
                None => true,
                Some(PluginValue::Bool(provides)) => *provides,
                other => {
                    return Err(runtime_throw(format!(
                        "getNames expects a bool provides flag, got {other:?}"
                    )));
                }
            };
            Ok(string_list(package.get_names(provides)))
        }
        "getId" => Ok(PluginValue::Int(package.get_id())),
        "isDev" => Ok(PluginValue::Bool(package.is_dev())),
        "getType" => Ok(PluginValue::string(package.get_type())),
        "getTargetDir" => Ok(optional_string(package.get_target_dir())),
        "getExtra" => Ok(string_keyed_map(package.get_extra())),
        "getInstallationSource" => Ok(optional_string(package.get_installation_source())),
        "getSourceType" => Ok(optional_string(package.get_source_type())),
        "getSourceUrl" => Ok(optional_string(package.get_source_url())),
        "getSourceUrls" => Ok(string_list(package.get_source_urls())),
        "getSourceReference" => Ok(optional_string(package.get_source_reference())),
        "getSourceMirrors" => Ok(mirror_list(package.get_source_mirrors())),
        "getDistType" => Ok(optional_string(package.get_dist_type())),
        "getDistUrl" => Ok(optional_string(package.get_dist_url())),
        "getDistUrls" => Ok(string_list(package.get_dist_urls())),
        "getDistReference" => Ok(optional_string(package.get_dist_reference())),
        "getDistSha1Checksum" => Ok(optional_string(package.get_dist_sha1_checksum())),
        "getDistMirrors" => Ok(mirror_list(package.get_dist_mirrors())),
        "getVersion" => Ok(PluginValue::string(package.get_version().to_string())),
        "getPrettyVersion" => Ok(PluginValue::string(
            package.get_pretty_version().to_string(),
        )),
        "getFullPrettyVersion" => {
            let truncate = match args.first() {
                None => true,
                Some(PluginValue::Bool(truncate)) => *truncate,
                other => {
                    return Err(runtime_throw(format!(
                        "getFullPrettyVersion expects a bool truncate flag, got {other:?}"
                    )));
                }
            };
            let display_mode = match args.get(1) {
                None | Some(PluginValue::Int(0)) => DisplayMode::SourceRefIfDev,
                Some(PluginValue::Int(1)) => DisplayMode::SourceRef,
                Some(PluginValue::Int(2)) => DisplayMode::DistRef,
                other => {
                    return Err(runtime_throw(format!(
                        "getFullPrettyVersion expects a display mode of 0..=2, got {other:?}"
                    )));
                }
            };
            Ok(PluginValue::string(
                package.get_full_pretty_version(truncate, display_mode),
            ))
        }
        "getStability" => Ok(PluginValue::string(package.get_stability().to_string())),
        "getRequires" => links(package.get_requires()),
        "getConflicts" => links(package.get_conflicts()),
        "getProvides" => links(package.get_provides()),
        "getReplaces" => links(package.get_replaces()),
        "getDevRequires" => links(package.get_dev_requires()),
        "getSuggests" => {
            let suggests = package.get_suggests();
            if suggests.is_empty() {
                Ok(PluginValue::List(Vec::new()))
            } else {
                Ok(PluginValue::Array(
                    suggests
                        .into_iter()
                        .map(|(name, description)| {
                            (name.into_bytes(), PluginValue::string(description))
                        })
                        .collect(),
                ))
            }
        }
        "getAutoload" => Ok(string_keyed_map(package.get_autoload())),
        "getDevAutoload" => Ok(string_keyed_map(package.get_dev_autoload())),
        "getIncludePaths" => Ok(string_list(package.get_include_paths())),
        "getPhpExt" => Ok(match package.get_php_ext() {
            Some(config) => string_keyed_map(config),
            None => PluginValue::Null,
        }),
        "getRepository" => match package.get_repository() {
            Some(repository) => repository_handle_value(&repository),
            None => Ok(PluginValue::Null),
        },
        "getBinaries" => Ok(string_list(package.get_binaries())),
        "getUniqueName" => Ok(PluginValue::string(package.get_unique_name())),
        "getNotificationUrl" => Ok(optional_string(package.get_notification_url())),
        "__toString" => Ok(PluginValue::string(package.get_unique_name())),
        "getPrettyString" => Ok(PluginValue::string(package.get_pretty_string())),
        "isDefaultBranch" => Ok(PluginValue::Bool(package.is_default_branch())),
        "getTransportOptions" => Ok(string_keyed_map(package.get_transport_options())),
        "getReleaseDate" => match package.get_release_date() {
            None => Ok(PluginValue::Null),
            // TODO(plugin): a \DateTimeInterface has to materialize as a real PHP object in the
            // child, which needs a snapshot encoding for value objects.
            Some(_) => Err(runtime_throw(
                "encoding the release date over RPC is not implemented yet".to_string(),
            )),
        },
        // TODO(plugin): the concrete-class surface below PackageInterface (`Package`'s setters,
        // `CompletePackage`'s metadata, `RootPackage`'s root-only state) is widened on demand,
        // driven by explicit errors from real plugins.
        other => Err(runtime_throw(format!(
            "the package method `{other}` is not available over RPC yet"
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
            let package = match args.first() {
                Some(PluginValue::RustHandle(handle)) => {
                    let entity = R_TABLE.with(|table| table.borrow().get(&handle.rhandle).cloned());
                    match entity {
                        Some(RustEntity::Package(package)) => {
                            PackageInterfaceHandle::from_rc_unchecked(package)
                        }
                        _ => {
                            return Err(runtime_throw(format!(
                                "getInstallPath expects a package handle, got Rust handle {}",
                                handle.rhandle
                            )));
                        }
                    }
                }
                other => {
                    return Err(runtime_throw(format!(
                        "getInstallPath expects a package argument, got {other:?}"
                    )));
                }
            };
            Ok(match im.borrow().get_install_path(package) {
                Some(path) => PluginValue::string(path),
                None => PluginValue::Null,
            })
        }
        "addInstaller" | "removeInstaller" => {
            let handle = match args.first() {
                Some(PluginValue::PhpHandle(handle)) => handle.clone(),
                other => {
                    return Err(runtime_throw(format!(
                        "{method_name} expects an installer object, got {other:?}"
                    )));
                }
            };
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
        "isInteractive" => Ok(PluginValue::Bool(io.borrow().is_interactive())),
        "isVerbose" => Ok(PluginValue::Bool(io.borrow().is_verbose())),
        "isVeryVerbose" => Ok(PluginValue::Bool(io.borrow().is_very_verbose())),
        "isDebug" => Ok(PluginValue::Bool(io.borrow().is_debug())),
        "isDecorated" => Ok(PluginValue::Bool(io.borrow().is_decorated())),
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
    // TODO(phase-e): lossy UTF-8; IO messages are bytes in PHP.
    let messages = match args.first() {
        Some(PluginValue::String(bytes)) => vec![String::from_utf8_lossy(bytes).into_owned()],
        Some(PluginValue::List(items)) => {
            let mut messages = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    PluginValue::String(bytes) => {
                        messages.push(String::from_utf8_lossy(bytes).into_owned());
                    }
                    other => {
                        return Err(runtime_throw(format!(
                            "{method_name} expects string messages, got {other:?}"
                        )));
                    }
                }
            }
            messages
        }
        other => {
            return Err(runtime_throw(format!(
                "{method_name} expects a string or list of strings, got {other:?}"
            )));
        }
    };
    let newline = match args.get(1) {
        Some(PluginValue::Bool(b)) => *b,
        None => true,
        other => {
            return Err(runtime_throw(format!(
                "{method_name} expects a bool newline flag, got {other:?}"
            )));
        }
    };
    let verbosity = match args.get(2) {
        Some(PluginValue::Int(v)) => *v,
        None => crate::io::NORMAL,
        other => {
            return Err(runtime_throw(format!(
                "{method_name} expects an int verbosity, got {other:?}"
            )));
        }
    };
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
            Err(throw) => Err(anyhow::anyhow!(shirabe_php_shim::RuntimeException {
                message: throw.message,
                code: throw.code,
            })),
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
            Err(throw) => Err(anyhow::anyhow!(shirabe_php_shim::RuntimeException {
                message: throw.message,
                code: throw.code,
            })),
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
                return Err(anyhow::anyhow!(shirabe_php_shim::RuntimeException {
                    message: throw.message,
                    code: throw.code,
                }));
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
                return Err(anyhow::anyhow!(shirabe_php_shim::RuntimeException {
                    message: throw.message,
                    code: throw.code,
                }));
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
        // TODO(phase-e): lossy UTF-8; event and method names are bytes in PHP.
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
    anyhow::anyhow!(shirabe_php_shim::RuntimeException {
        message: format!(
            "{class}::getSubscribedEvents() returned an unsupported shape over RPC: {value:?}"
        ),
        code: 0,
    })
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
    pub(crate) handle: PhpObjHandle,
}

impl PhpInstallerProxy {
    pub(crate) fn new(handle: PhpObjHandle) -> Self {
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

    fn package_arg(package: &PackageInterfaceHandle) -> anyhow::Result<PluginValue> {
        Ok(package_handle_value(package.as_rc())?)
    }

    fn optional_package_arg(
        package: &Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<PluginValue> {
        match package {
            Some(package) => Self::package_arg(package),
            None => Ok(PluginValue::Null),
        }
    }

    fn repo_arg(repo: &InstalledRepositoryInterfaceHandle) -> anyhow::Result<PluginValue> {
        Ok(repository_handle_value(&repo.as_repository_handle())?)
    }

    /// The `?PromiseInterface` half of the installer contract. A plugin installer that returns
    /// a real promise needs the promise machinery the RPC boundary does not carry yet, so it is
    /// an explicit error rather than a silently dropped continuation.
    fn promise_result(&self, method: &str, value: PluginValue) -> anyhow::Result<Option<PhpMixed>> {
        match value {
            PluginValue::Null => Ok(None),
            other => Err(anyhow::anyhow!(shirabe_php_shim::RuntimeException {
                message: format!(
                    "{}::{method}() returned a promise, which cannot cross the RPC boundary yet: {other:?}",
                    self.handle.class
                ),
                code: 0,
            })),
        }
    }

    fn unsupported_shape(&self, method: &str, value: &PluginValue) -> anyhow::Error {
        anyhow::anyhow!(shirabe_php_shim::RuntimeException {
            message: format!(
                "{}::{method}() returned an unsupported shape over RPC: {value:?}",
                self.handle.class
            ),
            code: 0,
        })
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
        let args = vec![Self::repo_arg(repo)?, Self::package_arg(&package)?];
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
            Self::package_arg(&package)?,
            Self::optional_package_arg(&prev_package)?,
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
            Self::package_arg(&package)?,
            Self::optional_package_arg(&prev_package)?,
        ];
        let value = self.call("prepare", args)?;
        self.promise_result("prepare", value)
    }

    async fn install(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![Self::repo_arg(repo)?, Self::package_arg(&package)?];
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
            Self::package_arg(&initial)?,
            Self::package_arg(&target)?,
        ];
        let value = self.call("update", args)?;
        self.promise_result("update", value)
    }

    async fn uninstall(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![Self::repo_arg(repo)?, Self::package_arg(&package)?];
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
            Self::package_arg(&package)?,
            Self::optional_package_arg(&prev_package)?,
        ];
        let value = self.call("cleanup", args)?;
        self.promise_result("cleanup", value)
    }

    fn get_install_path(&self, package: PackageInterfaceHandle) -> Option<String> {
        // PHP declares `getInstallPath(): string`; a failure here is a plugin error the
        // infallible signature cannot carry, so it aborts rather than answering a path that
        // would silently install the package in the wrong place.
        let args = vec![Self::package_arg(&package).unwrap_or_else(|error| {
            panic!(
                "{}::getInstallPath argument failed: {error:#}",
                self.handle.class
            )
        })];
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
    pub(crate) handle: PhpObjHandle,
}

impl PhpCapabilityProxy {
    pub(crate) fn new(handle: PhpObjHandle) -> Self {
        Self { handle }
    }
}

impl Capability for PhpCapabilityProxy {}

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
                return Err(anyhow::anyhow!(
                    shirabe_php_shim::UnexpectedValueException {
                        message: format!(
                            "Plugin capability {} failed to return an array from getCommands",
                            self.handle.class
                        ),
                        code: 0,
                    }
                ));
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
    anyhow::anyhow!(shirabe_php_shim::UnexpectedValueException {
        message: format!(
            "Plugin capability {} returned an invalid value, we expected an array of Composer\\Command\\BaseCommand objects",
            capability.class
        ),
        code: 0,
    })
}

impl Drop for PhpCommandProviderProxy {
    fn drop(&mut self) {
        let _ = release_php_handle(self.handle.phandle);
    }
}

/// Metadata row for one Rust-implemented command, mirrored into the worker as a
/// `\Shirabe\RustCommandStub` so a plugin-provided command can `find()` and invoke built-in
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
/// `Application::get_plugin_commands` once the full command set is known, booted in the worker
/// the first time a plugin-provided command actually runs.
#[derive(Debug)]
pub(crate) struct PhpConsoleApplicationContext {
    composer: ComposerHandle,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    initial_working_directory: Option<String>,
    disable_plugins_by_default: bool,
    disable_scripts_by_default: bool,
    rust_commands: Vec<RustCommandMetadata>,
    /// Clones of the plugin command handles; ownership (and release) stays with the
    /// `PhpCommandProxy` instances holding the originals.
    plugin_commands: Vec<PhpObjHandle>,
    app: std::cell::RefCell<Option<PhpObjHandle>>,
}

thread_local! {
    /// Handles of the `PhpCommandProxy` instances built while `Application::get_plugin_commands`
    /// collects providers; drained into the context it publishes.
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
    composer: &ComposerHandle,
    io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    initial_working_directory: Option<String>,
    disable_plugins_by_default: bool,
    disable_scripts_by_default: bool,
    rust_commands: Vec<RustCommandMetadata>,
    plugin_commands: Vec<PhpObjHandle>,
) {
    let context = std::rc::Rc::new(PhpConsoleApplicationContext {
        composer: composer.clone(),
        io: io.clone(),
        initial_working_directory,
        disable_plugins_by_default,
        disable_scripts_by_default,
        rust_commands,
        plugin_commands,
        app: std::cell::RefCell::new(None),
    });
    CONSOLE_APP_CONTEXT.with(|slot| *slot.borrow_mut() = Some(context));
}

impl PhpConsoleApplicationContext {
    /// Boots the worker-side application on first use and returns its handle.
    fn booted_app(&self) -> anyhow::Result<PhpObjHandle> {
        if let Some(app) = self.app.borrow().as_ref() {
            return Ok(app.clone());
        }
        let mut config: IndexMap<Vec<u8>, PluginValue> = IndexMap::new();
        config.insert(b"composer".to_vec(), composer_handle_value(&self.composer));
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
                return Err(anyhow::anyhow!(shirabe_php_shim::RuntimeException {
                    message: format!(
                        "__shirabe_console_application_boot returned an unsupported shape over RPC: {other:?}"
                    ),
                    code: 0,
                }));
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
    pub(crate) fn new(handle: PhpObjHandle) -> anyhow::Result<Self> {
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
        let proxy_command = match Self::call_metadata_getter(&handle, "isProxyCommand")? {
            PluginValue::Bool(proxy_command) => proxy_command,
            other => return Err(Self::unsupported_shape(&handle, "isProxyCommand", &other)),
        };
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
        use shirabe_external_packages::symfony::console::input::input_argument::InputArgument;
        use shirabe_external_packages::symfony::console::input::input_definition::{
            DefinitionItem, InputDefinition,
        };
        use shirabe_external_packages::symfony::console::input::input_option::InputOption;

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
            let default = field(&mut row, "default").to_php_mixed()?;
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
            let shortcut = field(&mut row, "shortcut").to_php_mixed()?;
            // `getDefault()` exposes the stored representation (`false` for VALUE_NONE), while
            // the constructor only accepts null there; mirror the constructor's normalization.
            let default = if accept_value {
                field(&mut row, "default").to_php_mixed()?
            } else {
                PhpMixed::Null
            };
            items.push(DefinitionItem::InputOption(InputOption::new(
                &name,
                shortcut,
                Some(mode),
                description,
                default,
            )?));
        }
        data.command_data().set_definition(
            shirabe_external_packages::symfony::console::command::command::SetDefinitionArg::Definition(
                InputDefinition::new(items)?,
            ),
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
        anyhow::anyhow!(shirabe_php_shim::RuntimeException {
            message: format!(
                "{}::{method}() returned an unsupported shape over RPC: {value:?}",
                handle.class
            ),
            code: 0,
        })
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
            .ok_or_else(|| {
                anyhow::anyhow!(shirabe_php_shim::RuntimeException {
                    message: format!(
                        "cannot run plugin-provided command {}: no worker-side console application context was published",
                        self.handle.class
                    ),
                    code: 0,
                })
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
        Err(anyhow::anyhow!(shirabe_php_shim::RuntimeException {
            message: format!(
                "plugin-provided command {} executes in the PHP worker through run(); execute() must not be called directly",
                self.handle.class
            ),
            code: 0,
        }))
    }

    fn is_proxy_command(&self) -> bool {
        self.proxy_command
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
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
