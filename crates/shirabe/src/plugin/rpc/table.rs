//! The R table: the Rust-side entities a PHP proxy stub points back to, the handles that name
//! them on the wire, and the PHP class each one is proxied as.

use crate::autoload::AutoloadGeneratorInterface;
use crate::composer::ComposerHandle;
use crate::config::Config;
use crate::dependency_resolver::AnyTransaction;
use crate::dependency_resolver::operation::AnyOperation;
use crate::downloader::DownloadManagerInterface;
use crate::event_dispatcher::{EventDispatcherInterface, EventInterface};
use crate::installer::InstallationManagerInterface;
use crate::io::{BufferIO, ConsoleIO, IOInterface, NullIO};
use crate::package::{AnyPackage, LockerInterface, PackageInterfaceHandle};
use crate::plugin::PluginInterface;
use crate::plugin::rpc::codec::arg_throw;
use crate::repository::{
    InstalledArrayRepository, InstalledFilesystemRepository, LockArrayRepository,
    RepositoryInterfaceHandle, RepositoryManagerInterface,
};
use crate::util::{Filesystem, HttpDownloader, ProcessExecutor};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_rpc::{FromWire, PhpThrow, PluginValue, RustObjHandle, ToWire, wire};
use shirabe_php_shim::{bconcat, php_str};

/// A Rust-side entity a PHP proxy stub points back to.
#[derive(Debug, Clone)]
pub(crate) enum RustEntity {
    Composer(ComposerHandle),
    AutoloadGenerator(std::rc::Rc<std::cell::RefCell<dyn AutoloadGeneratorInterface>>),
    Config(std::rc::Rc<std::cell::RefCell<Config>>),
    DownloadManager(std::rc::Rc<std::cell::RefCell<dyn DownloadManagerInterface>>),
    Filesystem(std::rc::Rc<std::cell::RefCell<Filesystem>>),
    Io(std::rc::Rc<std::cell::RefCell<dyn IOInterface>>),
    InstallationManager(std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>>),
    Locker(std::rc::Rc<std::cell::RefCell<dyn LockerInterface>>),
    RepositoryManager(std::rc::Rc<std::cell::RefCell<dyn RepositoryManagerInterface>>),
    Repository(RepositoryInterfaceHandle),
    Package(std::rc::Rc<std::cell::RefCell<AnyPackage>>),
    EventDispatcher(std::rc::Rc<std::cell::RefCell<dyn EventDispatcherInterface>>),
    Operation(std::rc::Rc<AnyOperation>),
    Transaction(std::rc::Rc<AnyTransaction>),
    Plugin(std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>),
    ProcessExecutor(std::rc::Rc<std::cell::RefCell<ProcessExecutor>>),
    HttpDownloader(std::rc::Rc<std::cell::RefCell<HttpDownloader>>),
    /// A child a `ProcessExecutor` job ran to its exit, behind a `Shirabe\RustProcessStub`.
    Process(std::rc::Rc<std::cell::RefCell<shirabe_symfony_process::Process>>),
    Event(std::rc::Rc<std::cell::RefCell<dyn EventInterface>>),
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
        RustEntity::Event(event) => std::rc::Rc::as_ptr(event) as *const () as usize,
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
    pub(crate) static R_TABLE: std::cell::RefCell<IndexMap<u64, RustEntity>> =
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
pub(crate) fn register_entity(entity: RustEntity) -> u64 {
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
pub(crate) fn config_handle_value(config: &std::rc::Rc<std::cell::RefCell<Config>>) -> PluginValue {
    let rhandle = register_entity(RustEntity::Config(config.clone()));
    rust_handle_value(rhandle, "Composer\\Config")
}

/// A `RustHandle` wire descriptor for a freshly registered (or re-interned) entity.
pub(crate) fn rust_handle_value(rhandle: u64, class: impl AsRef<[u8]>) -> PluginValue {
    PluginValue::RustHandle(RustObjHandle {
        rhandle,
        class: BString::from(class.as_ref()),
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
        Err(PhpThrow::runtime(
            "no proxy stub class is available for this repository over RPC yet",
        ))
    }
}

/// The proxy stub class matching a solver operation's concrete type.
pub(crate) fn operation_stub_class(operation: &AnyOperation) -> &'static BStr {
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
pub(crate) fn package_handle_value(
    package: &std::rc::Rc<std::cell::RefCell<AnyPackage>>,
) -> PluginValue {
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
    if any.downcast_ref::<BufferIO>().is_some() {
        Ok(BStr::new("Composer\\IO\\BufferIO"))
    } else if any.downcast_ref::<ConsoleIO>().is_some() {
        Ok(BStr::new("Composer\\IO\\ConsoleIO"))
    } else if any.downcast_ref::<NullIO>().is_some() {
        Ok(BStr::new("Composer\\IO\\NullIO"))
    } else {
        // TODO(plugin): only IO implementations with a generated proxy stub can cross the
        // boundary; the rest are an explicit error until stubs of their own are generated.
        Err(anyhow::anyhow!(
            "no proxy stub class is available for this IO implementation"
        ))
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

/// Wire value handing a Rust-side repository to the child, interned in the R table.
pub(crate) fn repository_handle_value(
    repository: &RepositoryInterfaceHandle,
) -> Result<PluginValue, PhpThrow> {
    let class = repository_stub_class(repository)?;
    let rhandle = register_entity(RustEntity::Repository(repository.clone()));
    Ok(rust_handle_value(rhandle, class))
}

/// A Rust-side entity a proxy stub stands for, as the `RustEntity` variant holding it.
pub(crate) trait FromRustEntity: Sized {
    /// What the value has to be, for the failure of a handle naming another kind of entity.
    const EXPECTED: &'static [u8];

    fn from_entity(entity: RustEntity) -> Option<Self>;
}

/// Resolves a handle descriptor back to the R-table entity it names.
fn rust_entity_from_wire<T: FromRustEntity>(value: &PluginValue) -> Result<T, PhpThrow> {
    let PluginValue::RustHandle(handle) = value else {
        return Err(wire::unexpected(T::EXPECTED, value));
    };
    R_TABLE
        .with(|table| table.borrow().get(&handle.rhandle).cloned())
        .and_then(T::from_entity)
        .ok_or_else(|| {
            PhpThrow::runtime(bconcat!(
                "expected ",
                T::EXPECTED,
                ", got Rust handle ",
                handle.rhandle
            ))
        })
}

/// Decodes the argument at `position` as an entity whose Rust type is a bare `Rc`, which only
/// `shirabe-php-rpc` can implement `FromWire` for.
pub(crate) fn entity_arg<T: FromRustEntity>(
    method: &BStr,
    args: &[PluginValue],
    position: usize,
) -> Result<T, PhpThrow> {
    rust_entity_from_wire(args.get(position).unwrap_or(&wire::NULL))
        .map_err(|throw| arg_throw(method, position, throw))
}

/// [`entity_arg`] for a nullable parameter.
pub(crate) fn optional_entity_arg<T: FromRustEntity>(
    method: &BStr,
    args: &[PluginValue],
    position: usize,
) -> Result<Option<T>, PhpThrow> {
    match args.get(position) {
        None | Some(PluginValue::Null) => Ok(None),
        Some(value) => rust_entity_from_wire(value)
            .map(Some)
            .map_err(|throw| arg_throw(method, position, throw)),
    }
}

impl FromRustEntity for PackageInterfaceHandle {
    const EXPECTED: &'static [u8] = b"a package";

    fn from_entity(entity: RustEntity) -> Option<Self> {
        match entity {
            RustEntity::Package(package) => {
                Some(PackageInterfaceHandle::from_rc_unchecked(package))
            }
            _ => None,
        }
    }
}

impl FromRustEntity for RepositoryInterfaceHandle {
    const EXPECTED: &'static [u8] = b"a repository";

    fn from_entity(entity: RustEntity) -> Option<Self> {
        match entity {
            RustEntity::Repository(repository) => Some(repository),
            _ => None,
        }
    }
}

impl FromRustEntity for std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
    const EXPECTED: &'static [u8] = b"an IOInterface";

    fn from_entity(entity: RustEntity) -> Option<Self> {
        match entity {
            RustEntity::Io(io) => Some(io),
            _ => None,
        }
    }
}

impl FromRustEntity for std::rc::Rc<std::cell::RefCell<Config>> {
    const EXPECTED: &'static [u8] = b"a Config";

    fn from_entity(entity: RustEntity) -> Option<Self> {
        match entity {
            RustEntity::Config(config) => Some(config),
            _ => None,
        }
    }
}

impl FromRustEntity for std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>> {
    const EXPECTED: &'static [u8] = b"an InstallationManager";

    fn from_entity(entity: RustEntity) -> Option<Self> {
        match entity {
            RustEntity::InstallationManager(im) => Some(im),
            _ => None,
        }
    }
}

impl FromRustEntity for std::rc::Rc<std::cell::RefCell<dyn LockerInterface>> {
    const EXPECTED: &'static [u8] = b"a Locker";

    fn from_entity(entity: RustEntity) -> Option<Self> {
        match entity {
            RustEntity::Locker(locker) => Some(locker),
            _ => None,
        }
    }
}

impl FromRustEntity for std::rc::Rc<std::cell::RefCell<ProcessExecutor>> {
    const EXPECTED: &'static [u8] = b"a ProcessExecutor";

    fn from_entity(entity: RustEntity) -> Option<Self> {
        match entity {
            RustEntity::ProcessExecutor(process) => Some(process),
            _ => None,
        }
    }
}

impl FromWire for PackageInterfaceHandle {
    fn from_wire(value: &PluginValue) -> Result<Self, PhpThrow> {
        rust_entity_from_wire(value)
    }
}

impl ToWire for PackageInterfaceHandle {
    fn to_wire(self) -> PluginValue {
        package_handle_value(self.as_rc())
    }
}

impl FromWire for RepositoryInterfaceHandle {
    fn from_wire(value: &PluginValue) -> Result<Self, PhpThrow> {
        rust_entity_from_wire(value)
    }
}
