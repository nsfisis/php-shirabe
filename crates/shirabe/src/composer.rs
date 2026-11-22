//! ref: composer/src/Composer/Composer.php sha1:aff4004371b3b4537982f01c4ec1397072390aa0
//! ref: composer/src/Composer/PartialComposer.php sha1:dc5eb5f37ab64d0b51dbb5bbe66c442e8a71b063

use crate::autoload::AutoloadGeneratorInterface;
use crate::config::Config;
use crate::downloader::DownloadManagerInterface;
use crate::event_dispatcher::EventDispatcherInterface;
use crate::installer::InstallationManagerInterface;
use crate::package::archiver::ArchiveManagerInterface;
use crate::package::{LockerInterface, RootPackageInterfaceHandle};
use crate::plugin::PluginManager;
use crate::repository::RepositoryManagerInterface;
use crate::util::r#loop::Loop;
use bstr::{BStr, BString};
use shirabe_php_binding_macros::php_binding;
use shirabe_php_shim::{PhpProp, bformat, php_regex, php_str, preg_is_match};

/// The Composer version this port tracks. Kept as-is so `Composer::VERSION`, the `composer`
/// platform package and the HTTP User-Agent keep reporting a value plugins and servers can
/// interpret. What Shirabe calls itself is `SHIRABE_VERSION`.
pub const VERSION: &BStr = php_str(b"2.9.8");
pub const BRANCH_ALIAS_VERSION: &BStr = php_str(b"");
pub const RELEASE_DATE: &BStr = php_str(b"2026-05-13 09:28:38");
pub const SOURCE_VERSION: &BStr = php_str(b"");
pub const RUNTIME_API_VERSION: &BStr = php_str(b"2.2.2");

/// What Shirabe reports as its own version: the package version of a release build, or
/// `<package version>+<commit hash>` for a development build, the way Composer's phar reports a
/// snapshot build through `getVersion()`. Baked in by `build.rs`.
pub const SHIRABE_VERSION: &BStr =
    php_str(include!(concat!(env!("OUT_DIR"), "/shirabe_version.rs")).as_bytes());

/// The package version of a development build, which plays the role of Composer's branch alias,
/// or empty for a release build. Baked in by `build.rs`.
pub const SHIRABE_BRANCH_ALIAS_VERSION: &BStr =
    php_str(include!(concat!(env!("OUT_DIR"), "/shirabe_branch_alias_version.rs")).as_bytes());

/// The UTC date of the commit this build was made from. Baked in by `build.rs`, the way Composer's
/// Compiler stamps `RELEASE_DATE` when it builds the phar.
pub const SHIRABE_RELEASE_DATE: &BStr =
    php_str(include!(concat!(env!("OUT_DIR"), "/release_date.rs")).as_bytes());

/// The deadline after which a development build reports itself as outdated, or `None` for a
/// release build. Baked in by `build.rs`.
///
/// Composer declares this as the global constant COMPOSER_DEV_WARNING_TIME from its phar stub, so
/// a plugin can read it back with `defined()`/`constant()`. Here it is a Rust constant, and
/// plugins cannot observe it. Technically speaking, it is incompatible with Composer, but trivial
/// enough.
pub const COMPOSER_DEV_WARNING_TIME: Option<i64> =
    include!(concat!(env!("OUT_DIR"), "/dev_warning_time.rs"));

pub fn get_version() -> BString {
    if VERSION == "@package_version@" {
        return BString::from(SOURCE_VERSION);
    }
    if !BRANCH_ALIAS_VERSION.is_empty() && preg_is_match(php_regex!("{^[a-f0-9]{40}$}"), VERSION) {
        return bformat!("{BRANCH_ALIAS_VERSION}+{VERSION}");
    }
    BString::from(VERSION)
}

/// Internal data type corresponding to \Composer\PartialComposer.
#[derive(Debug, Default)]
pub struct PartialComposer {
    global: PhpProp<bool>,
    package: PhpProp<Option<RootPackageInterfaceHandle>>,
    r#loop: PhpProp<Option<std::rc::Rc<std::cell::RefCell<Loop>>>>,
    repository_manager: PhpProp<Option<std::rc::Rc<dyn RepositoryManagerInterface>>>,
    installation_manager: PhpProp<Option<std::rc::Rc<dyn InstallationManagerInterface>>>,
    config: PhpProp<Option<std::rc::Rc<Config>>>,
    event_dispatcher: PhpProp<Option<std::rc::Rc<dyn EventDispatcherInterface>>>,
}

#[php_binding(r"Composer\PartialComposer", members = instance)]
impl PartialComposer {
    pub fn set_package(&self, package: RootPackageInterfaceHandle) {
        self.package.set(Some(package));
    }

    pub fn get_package(&self) -> RootPackageInterfaceHandle {
        self.package.cloned().unwrap()
    }

    pub fn set_config(&self, config: std::rc::Rc<Config>) {
        self.config.set(Some(config));
    }

    pub fn get_config(&self) -> std::rc::Rc<Config> {
        self.config.cloned().unwrap()
    }

    #[php(skip)]
    pub fn set_loop(&self, r#loop: std::rc::Rc<std::cell::RefCell<Loop>>) {
        self.r#loop.set(Some(r#loop));
    }

    #[php(skip)]
    pub fn get_loop(&self) -> std::rc::Rc<std::cell::RefCell<Loop>> {
        self.r#loop.cloned().unwrap()
    }

    pub fn set_repository_manager(&self, manager: std::rc::Rc<dyn RepositoryManagerInterface>) {
        self.repository_manager.set(Some(manager));
    }

    pub fn get_repository_manager(&self) -> std::rc::Rc<dyn RepositoryManagerInterface> {
        self.repository_manager.cloned().unwrap()
    }

    pub fn set_installation_manager(&self, manager: std::rc::Rc<dyn InstallationManagerInterface>) {
        self.installation_manager.set(Some(manager));
    }

    pub fn get_installation_manager(&self) -> std::rc::Rc<dyn InstallationManagerInterface> {
        self.installation_manager.cloned().unwrap()
    }

    pub fn set_event_dispatcher(
        &self,
        event_dispatcher: std::rc::Rc<dyn EventDispatcherInterface>,
    ) {
        self.event_dispatcher.set(Some(event_dispatcher));
    }

    pub fn get_event_dispatcher(&self) -> std::rc::Rc<dyn EventDispatcherInterface> {
        self.event_dispatcher.cloned().unwrap()
    }

    pub fn is_global(&self) -> bool {
        self.global.cloned()
    }

    pub fn set_global(&self) {
        self.global.set(true);
    }
}

/// Internal data type corresponding to \Composer\Composer.
#[derive(Debug)]
pub struct Composer {
    partial: PartialComposer,
    locker: PhpProp<Option<std::rc::Rc<dyn LockerInterface>>>,
    download_manager: PhpProp<Option<std::rc::Rc<dyn DownloadManagerInterface>>>,
    // TODO(plugin): plugin_manager is part of the plugin API
    plugin_manager: PhpProp<Option<std::rc::Rc<std::cell::RefCell<PluginManager>>>>,
    autoload_generator:
        PhpProp<Option<std::rc::Rc<std::cell::RefCell<dyn AutoloadGeneratorInterface>>>>,
    archive_manager: PhpProp<Option<std::rc::Rc<std::cell::RefCell<dyn ArchiveManagerInterface>>>>,
}

impl Default for Composer {
    fn default() -> Self {
        Self::new()
    }
}

#[php_binding(r"Composer\Composer", members = instance)]
impl Composer {
    pub fn new() -> Self {
        Self {
            partial: PartialComposer::default(),
            locker: PhpProp::new(None),
            download_manager: PhpProp::new(None),
            plugin_manager: PhpProp::new(None),
            autoload_generator: PhpProp::new(None),
            archive_manager: PhpProp::new(None),
        }
    }

    pub fn set_locker(&self, locker: std::rc::Rc<dyn LockerInterface>) {
        self.locker.set(Some(locker));
    }

    pub fn get_locker(&self) -> std::rc::Rc<dyn LockerInterface> {
        self.locker.cloned().unwrap()
    }

    pub fn set_download_manager(&self, manager: std::rc::Rc<dyn DownloadManagerInterface>) {
        self.download_manager.set(Some(manager));
    }

    pub fn get_download_manager(&self) -> std::rc::Rc<dyn DownloadManagerInterface> {
        self.download_manager.cloned().unwrap()
    }

    #[php(skip)]
    pub fn set_archive_manager(
        &self,
        manager: std::rc::Rc<std::cell::RefCell<dyn ArchiveManagerInterface>>,
    ) {
        self.archive_manager.set(Some(manager));
    }

    #[php(skip)]
    pub fn get_archive_manager(
        &self,
    ) -> std::rc::Rc<std::cell::RefCell<dyn ArchiveManagerInterface>> {
        self.archive_manager.cloned().unwrap()
    }

    // TODO(plugin): set_plugin_manager is part of the plugin API
    #[php(skip)]
    pub fn set_plugin_manager(&self, manager: std::rc::Rc<std::cell::RefCell<PluginManager>>) {
        self.plugin_manager.set(Some(manager));
    }

    // TODO(plugin): get_plugin_manager is part of the plugin API
    #[php(skip)]
    pub fn get_plugin_manager(&self) -> std::rc::Rc<std::cell::RefCell<PluginManager>> {
        self.plugin_manager.cloned().unwrap()
    }

    pub fn set_autoload_generator(
        &self,
        autoload_generator: std::rc::Rc<std::cell::RefCell<dyn AutoloadGeneratorInterface>>,
    ) {
        self.autoload_generator.set(Some(autoload_generator));
    }

    pub fn get_autoload_generator(
        &self,
    ) -> std::rc::Rc<std::cell::RefCell<dyn AutoloadGeneratorInterface>> {
        self.autoload_generator.cloned().unwrap()
    }

    pub fn as_partial(&self) -> &PartialComposer {
        &self.partial
    }

    pub fn set_package(&self, package: RootPackageInterfaceHandle) {
        self.partial.set_package(package);
    }

    pub fn get_package(&self) -> RootPackageInterfaceHandle {
        self.partial.get_package()
    }

    pub fn set_config(&self, config: std::rc::Rc<Config>) {
        self.partial.set_config(config);
    }

    pub fn get_config(&self) -> std::rc::Rc<Config> {
        self.partial.get_config()
    }

    #[php(skip)]
    pub fn set_loop(&self, r#loop: std::rc::Rc<std::cell::RefCell<Loop>>) {
        self.partial.set_loop(r#loop);
    }

    #[php(skip)]
    pub fn get_loop(&self) -> std::rc::Rc<std::cell::RefCell<Loop>> {
        self.partial.get_loop()
    }

    pub fn set_repository_manager(&self, manager: std::rc::Rc<dyn RepositoryManagerInterface>) {
        self.partial.set_repository_manager(manager);
    }

    pub fn get_repository_manager(&self) -> std::rc::Rc<dyn RepositoryManagerInterface> {
        self.partial.get_repository_manager()
    }

    pub fn set_installation_manager(&self, manager: std::rc::Rc<dyn InstallationManagerInterface>) {
        self.partial.set_installation_manager(manager);
    }

    pub fn get_installation_manager(&self) -> std::rc::Rc<dyn InstallationManagerInterface> {
        self.partial.get_installation_manager()
    }

    pub fn set_event_dispatcher(&self, dispatcher: std::rc::Rc<dyn EventDispatcherInterface>) {
        self.partial.set_event_dispatcher(dispatcher);
    }

    pub fn get_event_dispatcher(&self) -> std::rc::Rc<dyn EventDispatcherInterface> {
        self.partial.get_event_dispatcher()
    }

    pub fn is_global(&self) -> bool {
        self.partial.is_global()
    }

    pub fn set_global(&self) {
        self.partial.set_global();
    }
}

#[derive(Debug)]
pub enum PartialOrFullComposer {
    Full(Composer),
    Partial(PartialComposer),
}

impl PartialOrFullComposer {
    pub fn new_full() -> Self {
        Self::Full(Composer::new())
    }

    pub fn new_partial() -> Self {
        Self::Partial(PartialComposer::default())
    }

    pub fn is_full(&self) -> bool {
        matches!(self, Self::Full(_))
    }

    pub fn is_partial(&self) -> bool {
        matches!(self, Self::Partial(_))
    }

    pub fn as_full(&self) -> Option<&Composer> {
        match self {
            Self::Full(full) => Some(full),
            Self::Partial(_) => None,
        }
    }

    pub fn as_partial(&self) -> &PartialComposer {
        match self {
            Self::Full(full) => full.as_partial(),
            Self::Partial(partial) => partial,
        }
    }

    pub fn set_package(&self, package: RootPackageInterfaceHandle) {
        match self {
            Self::Full(full) => full.set_package(package),
            Self::Partial(partial) => partial.set_package(package),
        }
    }

    pub fn get_package(&self) -> RootPackageInterfaceHandle {
        match self {
            Self::Full(full) => full.get_package(),
            Self::Partial(partial) => partial.get_package(),
        }
    }

    pub fn set_config(&self, config: std::rc::Rc<Config>) {
        match self {
            Self::Full(full) => full.set_config(config),
            Self::Partial(partial) => partial.set_config(config),
        }
    }

    pub fn get_config(&self) -> std::rc::Rc<Config> {
        match self {
            Self::Full(full) => full.get_config(),
            Self::Partial(partial) => partial.get_config(),
        }
    }

    pub fn set_loop(&self, r#loop: std::rc::Rc<std::cell::RefCell<Loop>>) {
        match self {
            Self::Full(full) => full.set_loop(r#loop),
            Self::Partial(partial) => partial.set_loop(r#loop),
        }
    }

    pub fn get_loop(&self) -> std::rc::Rc<std::cell::RefCell<Loop>> {
        match self {
            Self::Full(full) => full.get_loop(),
            Self::Partial(partial) => partial.get_loop(),
        }
    }

    pub fn set_repository_manager(&self, manager: std::rc::Rc<dyn RepositoryManagerInterface>) {
        match self {
            Self::Full(full) => full.set_repository_manager(manager),
            Self::Partial(partial) => partial.set_repository_manager(manager),
        }
    }

    pub fn get_repository_manager(&self) -> std::rc::Rc<dyn RepositoryManagerInterface> {
        match self {
            Self::Full(full) => full.get_repository_manager(),
            Self::Partial(partial) => partial.get_repository_manager(),
        }
    }

    pub fn set_installation_manager(&self, manager: std::rc::Rc<dyn InstallationManagerInterface>) {
        match self {
            Self::Full(full) => full.set_installation_manager(manager),
            Self::Partial(partial) => partial.set_installation_manager(manager),
        }
    }

    pub fn get_installation_manager(&self) -> std::rc::Rc<dyn InstallationManagerInterface> {
        match self {
            Self::Full(full) => full.get_installation_manager(),
            Self::Partial(partial) => partial.get_installation_manager(),
        }
    }

    pub fn set_event_dispatcher(&self, dispatcher: std::rc::Rc<dyn EventDispatcherInterface>) {
        match self {
            Self::Full(full) => full.set_event_dispatcher(dispatcher),
            Self::Partial(partial) => partial.set_event_dispatcher(dispatcher),
        }
    }

    pub fn get_event_dispatcher(&self) -> std::rc::Rc<dyn EventDispatcherInterface> {
        match self {
            Self::Full(full) => full.get_event_dispatcher(),
            Self::Partial(partial) => partial.get_event_dispatcher(),
        }
    }

    pub fn is_global(&self) -> bool {
        match self {
            Self::Full(full) => full.is_global(),
            Self::Partial(partial) => partial.is_global(),
        }
    }

    pub fn set_global(&self) {
        match self {
            Self::Full(full) => full.set_global(),
            Self::Partial(partial) => partial.set_global(),
        }
    }
}

/// Shared reference to \Composer\PartialComposer or \Composer\Composer.
/// Use this for parameters or fields typed as \Composer\PartialComposer in PHP.
#[derive(Debug, Clone)]
pub struct PartialComposerHandle(std::rc::Rc<PartialOrFullComposer>);

impl PartialComposerHandle {
    pub fn is_full(&self) -> bool {
        self.0.is_full()
    }

    /// Downcast to a full Composer handle. PHP `$composer instanceof Composer`.
    pub fn as_full(&self) -> Option<ComposerHandle> {
        if self.0.is_full() {
            Some(ComposerHandle::from_rc_unchecked(self.0.clone()))
        } else {
            None
        }
    }

    pub fn downgrade(&self) -> PartialComposerWeakHandle {
        PartialComposerWeakHandle(std::rc::Rc::downgrade(&self.0))
    }

    pub fn from_rc(rc: std::rc::Rc<PartialOrFullComposer>) -> Self {
        Self(rc)
    }

    pub fn as_rc(&self) -> &std::rc::Rc<PartialOrFullComposer> {
        &self.0
    }
}

/// Shared weak reference to \Composer\PartialComposer or \Composer\Composer.
#[derive(Debug, Clone)]
pub struct PartialComposerWeakHandle(std::rc::Weak<PartialOrFullComposer>);

impl PartialComposerWeakHandle {
    pub fn upgrade(&self) -> Option<PartialComposerHandle> {
        self.0.upgrade().map(PartialComposerHandle)
    }

    pub fn from_weak(weak: std::rc::Weak<PartialOrFullComposer>) -> Self {
        Self(weak)
    }
}

/// Shared reference to \Composer\Composer.
/// Use this for parameters or fields typed as \Composer\Composer in PHP.
#[derive(Debug, Clone)]
pub struct ComposerHandle(std::rc::Rc<PartialOrFullComposer>);

impl ComposerHandle {
    pub fn upcast(&self) -> PartialComposerHandle {
        PartialComposerHandle::from_rc(self.0.clone())
    }

    pub fn downgrade(&self) -> ComposerWeakHandle {
        ComposerWeakHandle(std::rc::Rc::downgrade(&self.0))
    }

    pub fn from_rc_unchecked(rc: std::rc::Rc<PartialOrFullComposer>) -> Self {
        Self(rc)
    }

    pub fn as_rc(&self) -> &std::rc::Rc<PartialOrFullComposer> {
        &self.0
    }
}

impl std::ops::Deref for PartialComposerHandle {
    type Target = PartialComposer;

    fn deref(&self) -> &PartialComposer {
        self.0.as_partial()
    }
}

impl std::ops::Deref for ComposerHandle {
    type Target = Composer;

    fn deref(&self) -> &Composer {
        self.0
            .as_full()
            .expect("Composer handle invariant: inner is Full")
    }
}

impl From<ComposerHandle> for PartialComposerHandle {
    fn from(c: ComposerHandle) -> Self {
        c.upcast()
    }
}

/// Shared weak reference to \Composer\Composer.
#[derive(Debug, Clone)]
pub struct ComposerWeakHandle(std::rc::Weak<PartialOrFullComposer>);

impl ComposerWeakHandle {
    pub fn upgrade(&self) -> Option<ComposerHandle> {
        self.0.upgrade().map(ComposerHandle)
    }

    pub fn from_weak(weak: std::rc::Weak<PartialOrFullComposer>) -> Self {
        Self(weak)
    }
}

/// Borrows a polymorphic `PartialComposer` as a fully-loaded `Composer`.
///
/// Commands obtain their Composer through `require_composer` / `create_composer_instance`,
/// which always yield a fully-loaded instance, so the downcast is infallible here.
pub fn composer_full(composer: &PartialComposerHandle) -> &Composer {
    composer
        .as_rc()
        .as_full()
        .expect("a fully loaded Composer is required here")
}
