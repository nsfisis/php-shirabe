//! ref: composer/src/Composer/Composer.php
//! ref: composer/src/Composer/PartialComposer.php

use shirabe_external_packages::composer::pcre::Preg;

use crate::autoload::AutoloadGenerator;
use crate::config::Config;
use crate::downloader::DownloadManager;
use crate::event_dispatcher::EventDispatcher;
use crate::installer::InstallationManager;
use crate::package::archiver::ArchiveManager;
use crate::package::{Locker, RootPackageInterfaceHandle};
use crate::plugin::PluginManager;
use crate::repository::RepositoryManager;
use crate::util::r#loop::Loop;

// TODO: change this information to Shirabe version.
pub const VERSION: &str = "2.9.7";
pub const BRANCH_ALIAS_VERSION: &str = "";
pub const RELEASE_DATE: &str = "2026-04-14 13:31:52";
pub const SOURCE_VERSION: &str = "";
pub const RUNTIME_API_VERSION: &str = "2.2.2";

pub fn get_version() -> String {
    if VERSION == "@package_version@" {
        return SOURCE_VERSION.to_string();
    }
    if !BRANCH_ALIAS_VERSION.is_empty() && Preg::is_match("{^[a-f0-9]{40}$}", VERSION) {
        return format!("{}+{}", BRANCH_ALIAS_VERSION, VERSION);
    }
    VERSION.to_string()
}

/// Internal data type corresponding to \Composer\PartialComposer.
#[derive(Debug, Default)]
pub struct PartialComposer {
    global: bool,
    package: Option<RootPackageInterfaceHandle>,
    r#loop: Option<std::rc::Rc<std::cell::RefCell<Loop>>>,
    repository_manager: Option<std::rc::Rc<std::cell::RefCell<RepositoryManager>>>,
    installation_manager: Option<std::rc::Rc<std::cell::RefCell<InstallationManager>>>,
    config: Option<std::rc::Rc<std::cell::RefCell<Config>>>,
    event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
}

impl PartialComposer {
    pub fn set_package(&mut self, package: RootPackageInterfaceHandle) {
        self.package = Some(package);
    }

    pub fn get_package(&self) -> &RootPackageInterfaceHandle {
        self.package.as_ref().unwrap()
    }

    pub fn set_config(&mut self, config: std::rc::Rc<std::cell::RefCell<Config>>) {
        self.config = Some(config);
    }

    pub fn get_config(&self) -> std::rc::Rc<std::cell::RefCell<Config>> {
        self.config.as_ref().unwrap().clone()
    }

    pub fn set_loop(&mut self, r#loop: std::rc::Rc<std::cell::RefCell<Loop>>) {
        self.r#loop = Some(r#loop);
    }

    pub fn get_loop(&self) -> std::rc::Rc<std::cell::RefCell<Loop>> {
        self.r#loop.as_ref().unwrap().clone()
    }

    pub fn set_repository_manager(
        &mut self,
        manager: std::rc::Rc<std::cell::RefCell<RepositoryManager>>,
    ) {
        self.repository_manager = Some(manager);
    }

    pub fn get_repository_manager(&self) -> std::rc::Rc<std::cell::RefCell<RepositoryManager>> {
        self.repository_manager.as_ref().unwrap().clone()
    }

    pub fn set_installation_manager(
        &mut self,
        manager: std::rc::Rc<std::cell::RefCell<InstallationManager>>,
    ) {
        self.installation_manager = Some(manager);
    }

    pub fn get_installation_manager(&self) -> std::rc::Rc<std::cell::RefCell<InstallationManager>> {
        self.installation_manager.as_ref().unwrap().clone()
    }

    pub fn set_event_dispatcher(
        &mut self,
        event_dispatcher: std::rc::Rc<std::cell::RefCell<EventDispatcher>>,
    ) {
        self.event_dispatcher = Some(event_dispatcher);
    }

    pub fn get_event_dispatcher(&self) -> std::rc::Rc<std::cell::RefCell<EventDispatcher>> {
        self.event_dispatcher.as_ref().unwrap().clone()
    }

    pub fn is_global(&self) -> bool {
        self.global
    }

    pub fn set_global(&mut self) {
        self.global = true;
    }
}

/// Internal data type corresponding to \Composer\Composer.
#[derive(Debug)]
pub struct Composer {
    partial: PartialComposer,
    locker: Option<std::rc::Rc<std::cell::RefCell<Locker>>>,
    download_manager: Option<std::rc::Rc<std::cell::RefCell<DownloadManager>>>,
    // TODO(plugin): plugin_manager is part of the plugin API
    plugin_manager: Option<std::rc::Rc<std::cell::RefCell<PluginManager>>>,
    autoload_generator: Option<std::rc::Rc<std::cell::RefCell<AutoloadGenerator>>>,
    archive_manager: Option<std::rc::Rc<std::cell::RefCell<ArchiveManager>>>,
}

impl Default for Composer {
    fn default() -> Self {
        Self::new()
    }
}

impl Composer {
    pub fn new() -> Self {
        Self {
            partial: PartialComposer::default(),
            locker: None,
            download_manager: None,
            plugin_manager: None,
            autoload_generator: None,
            archive_manager: None,
        }
    }

    pub fn set_locker(&mut self, locker: std::rc::Rc<std::cell::RefCell<Locker>>) {
        self.locker = Some(locker);
    }

    pub fn get_locker(&self) -> std::rc::Rc<std::cell::RefCell<Locker>> {
        self.locker.as_ref().unwrap().clone()
    }

    pub fn set_download_manager(
        &mut self,
        manager: std::rc::Rc<std::cell::RefCell<DownloadManager>>,
    ) {
        self.download_manager = Some(manager);
    }

    pub fn get_download_manager(&self) -> std::rc::Rc<std::cell::RefCell<DownloadManager>> {
        self.download_manager.as_ref().unwrap().clone()
    }

    pub fn set_archive_manager(
        &mut self,
        manager: std::rc::Rc<std::cell::RefCell<ArchiveManager>>,
    ) {
        self.archive_manager = Some(manager);
    }

    pub fn get_archive_manager(&self) -> std::rc::Rc<std::cell::RefCell<ArchiveManager>> {
        self.archive_manager.as_ref().unwrap().clone()
    }

    // TODO(plugin): set_plugin_manager is part of the plugin API
    pub fn set_plugin_manager(&mut self, manager: std::rc::Rc<std::cell::RefCell<PluginManager>>) {
        self.plugin_manager = Some(manager);
    }

    // TODO(plugin): get_plugin_manager is part of the plugin API
    pub fn get_plugin_manager(&self) -> std::rc::Rc<std::cell::RefCell<PluginManager>> {
        self.plugin_manager.as_ref().unwrap().clone()
    }

    pub fn set_autoload_generator(
        &mut self,
        autoload_generator: std::rc::Rc<std::cell::RefCell<AutoloadGenerator>>,
    ) {
        self.autoload_generator = Some(autoload_generator);
    }

    pub fn get_autoload_generator(&self) -> std::rc::Rc<std::cell::RefCell<AutoloadGenerator>> {
        self.autoload_generator.as_ref().unwrap().clone()
    }

    pub fn as_partial(&self) -> &PartialComposer {
        &self.partial
    }

    pub fn as_partial_mut(&mut self) -> &mut PartialComposer {
        &mut self.partial
    }

    pub fn set_package(&mut self, package: RootPackageInterfaceHandle) {
        self.partial.set_package(package);
    }

    pub fn get_package(&self) -> &RootPackageInterfaceHandle {
        self.partial.get_package()
    }

    pub fn set_config(&mut self, config: std::rc::Rc<std::cell::RefCell<crate::config::Config>>) {
        self.partial.set_config(config);
    }

    pub fn get_config(&self) -> std::rc::Rc<std::cell::RefCell<crate::config::Config>> {
        self.partial.get_config()
    }

    pub fn set_loop(&mut self, r#loop: std::rc::Rc<std::cell::RefCell<crate::util::r#loop::Loop>>) {
        self.partial.set_loop(r#loop);
    }

    pub fn get_loop(&self) -> std::rc::Rc<std::cell::RefCell<crate::util::r#loop::Loop>> {
        self.partial.get_loop()
    }

    pub fn set_repository_manager(
        &mut self,
        manager: std::rc::Rc<std::cell::RefCell<crate::repository::RepositoryManager>>,
    ) {
        self.partial.set_repository_manager(manager);
    }

    pub fn get_repository_manager(
        &self,
    ) -> std::rc::Rc<std::cell::RefCell<crate::repository::RepositoryManager>> {
        self.partial.get_repository_manager()
    }

    pub fn set_installation_manager(
        &mut self,
        manager: std::rc::Rc<std::cell::RefCell<crate::installer::InstallationManager>>,
    ) {
        self.partial.set_installation_manager(manager);
    }

    pub fn get_installation_manager(
        &self,
    ) -> std::rc::Rc<std::cell::RefCell<crate::installer::InstallationManager>> {
        self.partial.get_installation_manager()
    }

    pub fn set_event_dispatcher(
        &mut self,
        dispatcher: std::rc::Rc<std::cell::RefCell<crate::event_dispatcher::EventDispatcher>>,
    ) {
        self.partial.set_event_dispatcher(dispatcher);
    }

    pub fn get_event_dispatcher(
        &self,
    ) -> std::rc::Rc<std::cell::RefCell<crate::event_dispatcher::EventDispatcher>> {
        self.partial.get_event_dispatcher()
    }

    pub fn is_global(&self) -> bool {
        self.partial.is_global()
    }

    pub fn set_global(&mut self) {
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

    pub fn as_full_mut(&mut self) -> Option<&mut Composer> {
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

    pub fn as_partial_mut(&mut self) -> &mut PartialComposer {
        match self {
            Self::Full(full) => full.as_partial_mut(),
            Self::Partial(partial) => partial,
        }
    }

    pub fn set_package(&mut self, package: RootPackageInterfaceHandle) {
        match self {
            Self::Full(full) => full.set_package(package),
            Self::Partial(partial) => partial.set_package(package),
        }
    }

    pub fn get_package(&self) -> &RootPackageInterfaceHandle {
        match self {
            Self::Full(full) => full.get_package(),
            Self::Partial(partial) => partial.get_package(),
        }
    }

    pub fn set_config(&mut self, config: std::rc::Rc<std::cell::RefCell<crate::config::Config>>) {
        match self {
            Self::Full(full) => full.set_config(config),
            Self::Partial(partial) => partial.set_config(config),
        }
    }

    pub fn get_config(&self) -> std::rc::Rc<std::cell::RefCell<crate::config::Config>> {
        match self {
            Self::Full(full) => full.get_config(),
            Self::Partial(partial) => partial.get_config(),
        }
    }

    pub fn set_loop(&mut self, r#loop: std::rc::Rc<std::cell::RefCell<crate::util::r#loop::Loop>>) {
        match self {
            Self::Full(full) => full.set_loop(r#loop),
            Self::Partial(partial) => partial.set_loop(r#loop),
        }
    }

    pub fn get_loop(&self) -> std::rc::Rc<std::cell::RefCell<crate::util::r#loop::Loop>> {
        match self {
            Self::Full(full) => full.get_loop(),
            Self::Partial(partial) => partial.get_loop(),
        }
    }

    pub fn set_repository_manager(
        &mut self,
        manager: std::rc::Rc<std::cell::RefCell<crate::repository::RepositoryManager>>,
    ) {
        match self {
            Self::Full(full) => full.set_repository_manager(manager),
            Self::Partial(partial) => partial.set_repository_manager(manager),
        }
    }

    pub fn get_repository_manager(
        &self,
    ) -> std::rc::Rc<std::cell::RefCell<crate::repository::RepositoryManager>> {
        match self {
            Self::Full(full) => full.get_repository_manager(),
            Self::Partial(partial) => partial.get_repository_manager(),
        }
    }

    pub fn set_installation_manager(
        &mut self,
        manager: std::rc::Rc<std::cell::RefCell<crate::installer::InstallationManager>>,
    ) {
        match self {
            Self::Full(full) => full.set_installation_manager(manager),
            Self::Partial(partial) => partial.set_installation_manager(manager),
        }
    }

    pub fn get_installation_manager(
        &self,
    ) -> std::rc::Rc<std::cell::RefCell<crate::installer::InstallationManager>> {
        match self {
            Self::Full(full) => full.get_installation_manager(),
            Self::Partial(partial) => partial.get_installation_manager(),
        }
    }

    pub fn set_event_dispatcher(
        &mut self,
        dispatcher: std::rc::Rc<std::cell::RefCell<crate::event_dispatcher::EventDispatcher>>,
    ) {
        match self {
            Self::Full(full) => full.set_event_dispatcher(dispatcher),
            Self::Partial(partial) => partial.set_event_dispatcher(dispatcher),
        }
    }

    pub fn get_event_dispatcher(
        &self,
    ) -> std::rc::Rc<std::cell::RefCell<crate::event_dispatcher::EventDispatcher>> {
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

    pub fn set_global(&mut self) {
        match self {
            Self::Full(full) => full.set_global(),
            Self::Partial(partial) => partial.set_global(),
        }
    }
}

/// Shared reference to \Composer\PartialComposer or \Composer\Composer.
/// Use this for parameters or fields typed as \Composer\PartialComposer in PHP.
#[derive(Debug, Clone)]
pub struct PartialComposerHandle(std::rc::Rc<std::cell::RefCell<PartialOrFullComposer>>);

impl PartialComposerHandle {
    pub fn borrow_partial(&self) -> std::cell::Ref<'_, PartialComposer> {
        std::cell::Ref::map(self.0.borrow(), |c| c.as_partial())
    }

    pub fn borrow_partial_mut(&self) -> std::cell::RefMut<'_, PartialComposer> {
        std::cell::RefMut::map(self.0.borrow_mut(), |c| c.as_partial_mut())
    }

    pub fn is_full(&self) -> bool {
        self.0.borrow().is_full()
    }

    /// Downcast to a full Composer handle. PHP `$composer instanceof Composer`.
    pub fn as_full(&self) -> Option<ComposerHandle> {
        if self.0.borrow().is_full() {
            Some(ComposerHandle::from_rc_unchecked(self.0.clone()))
        } else {
            None
        }
    }

    pub fn downgrade(&self) -> PartialComposerWeakHandle {
        PartialComposerWeakHandle(std::rc::Rc::downgrade(&self.0))
    }

    pub fn from_rc(rc: std::rc::Rc<std::cell::RefCell<PartialOrFullComposer>>) -> Self {
        Self(rc)
    }

    pub fn as_rc(&self) -> &std::rc::Rc<std::cell::RefCell<PartialOrFullComposer>> {
        &self.0
    }
}

/// Shared weak reference to \Composer\PartialComposer or \Composer\Composer.
#[derive(Debug, Clone)]
pub struct PartialComposerWeakHandle(std::rc::Weak<std::cell::RefCell<PartialOrFullComposer>>);

impl PartialComposerWeakHandle {
    pub fn upgrade(&self) -> Option<PartialComposerHandle> {
        self.0.upgrade().map(PartialComposerHandle)
    }

    pub fn from_weak(weak: std::rc::Weak<std::cell::RefCell<PartialOrFullComposer>>) -> Self {
        Self(weak)
    }
}

/// Shared reference to \Composer\Composer.
/// Use this for parameters or fields typed as \Composer\Composer in PHP.
#[derive(Debug, Clone)]
pub struct ComposerHandle(std::rc::Rc<std::cell::RefCell<PartialOrFullComposer>>);

impl ComposerHandle {
    pub fn borrow(&self) -> std::cell::Ref<'_, Composer> {
        std::cell::Ref::map(self.0.borrow(), |c| {
            c.as_full()
                .expect("Composer handle invariant: inner is Full")
        })
    }

    pub fn borrow_mut(&self) -> std::cell::RefMut<'_, Composer> {
        std::cell::RefMut::map(self.0.borrow_mut(), |c| {
            c.as_full_mut()
                .expect("Composer handle invariant: inner is Full")
        })
    }

    pub fn upcast(&self) -> PartialComposerHandle {
        PartialComposerHandle::from_rc(self.0.clone())
    }

    pub fn downgrade(&self) -> ComposerWeakHandle {
        ComposerWeakHandle(std::rc::Rc::downgrade(&self.0))
    }

    pub fn from_rc_unchecked(rc: std::rc::Rc<std::cell::RefCell<PartialOrFullComposer>>) -> Self {
        Self(rc)
    }

    pub fn as_rc(&self) -> &std::rc::Rc<std::cell::RefCell<PartialOrFullComposer>> {
        &self.0
    }
}

impl From<ComposerHandle> for PartialComposerHandle {
    fn from(c: ComposerHandle) -> Self {
        c.upcast()
    }
}

/// Shared weak reference to \Composer\Composer.
#[derive(Debug, Clone)]
pub struct ComposerWeakHandle(std::rc::Weak<std::cell::RefCell<PartialOrFullComposer>>);

impl ComposerWeakHandle {
    pub fn upgrade(&self) -> Option<ComposerHandle> {
        self.0.upgrade().map(ComposerHandle)
    }

    pub fn from_weak(weak: std::rc::Weak<std::cell::RefCell<PartialOrFullComposer>>) -> Self {
        Self(weak)
    }
}

/// Borrows a polymorphic `PartialComposer` as a fully-loaded `Composer`.
///
/// Commands obtain their Composer through `require_composer` / `create_composer_instance`,
/// which always yield a fully-loaded instance, so the downcast is infallible here.
pub fn composer_full(composer: &PartialComposerHandle) -> std::cell::Ref<'_, Composer> {
    std::cell::Ref::map(composer.as_rc().borrow(), |c| {
        c.as_full()
            .expect("a fully loaded Composer is required here")
    })
}

/// Mutably borrows a polymorphic `PartialComposer` as a fully-loaded `Composer`. See [`composer_full`].
pub fn composer_full_mut(composer: &PartialComposerHandle) -> std::cell::RefMut<'_, Composer> {
    std::cell::RefMut::map(composer.as_rc().borrow_mut(), |c| {
        c.as_full_mut()
            .expect("a fully loaded Composer is required here")
    })
}
