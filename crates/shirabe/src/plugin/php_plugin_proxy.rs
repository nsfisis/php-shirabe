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
    InstalledArrayRepository, InstalledFilesystemRepository, RepositoryInterfaceHandle,
    RepositoryManagerInterface,
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
            None => Err(runtime_throw(format!("unknown Rust handle {rhandle}"))),
        }
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
            package_handle_value(&package)
        }
        // TODO(plugin): the remaining Composer object graph (getConfig, getLocker, ...)
        // becomes reachable over RPC on demand, driven by explicit errors from real plugins.
        other => Err(runtime_throw(format!(
            "the Composer method `{other}` is not available over RPC yet"
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

fn dispatch_package_method(
    package: &std::rc::Rc<std::cell::RefCell<AnyPackage>>,
    method_name: &str,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let package = package.borrow();
    let package = package.as_package_interface();
    match method_name {
        "getName" => Ok(PluginValue::string(package.get_name().to_string())),
        "getType" => Ok(PluginValue::string(package.get_type())),
        "getPrettyVersion" => Ok(PluginValue::string(
            package.get_pretty_version().to_string(),
        )),
        "getExtra" => Ok(PluginValue::from_php_mixed(&PhpMixed::Array(
            package.get_extra(),
        ))),
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
        "getRequires" => {
            let requires = package.get_requires();
            if requires.is_empty() {
                // An empty PHP array crosses the wire as a list.
                Ok(PluginValue::List(Vec::new()))
            } else {
                // TODO(plugin): Link is a rust-snapshot value whose constraint field must
                // materialize as a real composer/semver object in the child; the snapshot
                // encoding does not exist yet.
                Err(runtime_throw(
                    "encoding Link values over RPC is not implemented yet".to_string(),
                ))
            }
        }
        // TODO(plugin): the remaining PackageInterface surface (setters included) is widened
        // on demand, driven by explicit errors from real plugins.
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

/// `BaseCommand` adapter for a command entity living in the PHP child process. The Rust-side
/// command state mirrors the child's list metadata (name, description, aliases, hidden flag —
/// read back over RPC at construction, after the PHP constructor ran `configure()`); running
/// the command needs the PHP-side Symfony Application and is an explicit error until that
/// exists.
#[derive(Debug)]
pub struct PhpCommandProxy {
    base_command_data: crate::command::BaseCommandData,
    handle: PhpObjHandle,
}

impl PhpCommandProxy {
    pub(crate) fn new(handle: PhpObjHandle) -> anyhow::Result<Self> {
        let data = crate::command::BaseCommandData::new(None);
        // TODO(plugin): the input definition (arguments/options) is not read back yet;
        // `help` rendering and input parsing for this command need it.
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
        Ok(Self {
            base_command_data: data,
            handle,
        })
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
    fn execute(
        &self,
        _input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        // TODO(plugin): executing a plugin-provided command requires the PHP-side Symfony
        // Application; until then this is an explicit error, never a silent no-op.
        Err(anyhow::anyhow!(shirabe_php_shim::RuntimeException {
            message: format!(
                "cannot execute plugin-provided command {} yet: running PHP commands is not supported",
                self.handle.class
            ),
            code: 0,
        }))
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
