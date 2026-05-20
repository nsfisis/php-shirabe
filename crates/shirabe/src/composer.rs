//! ref: composer/src/Composer/Composer.php

use shirabe_external_packages::composer::pcre::Preg;

use crate::autoload::AutoloadGenerator;
use crate::downloader::DownloadManager;
use crate::package::Locker;
use crate::package::archiver::ArchiveManager;
use crate::partial_composer::PartialComposer;
use crate::plugin::PluginManager;

#[derive(Debug)]
pub struct Composer {
    inner: PartialComposer,
    locker: Option<Locker>,
    download_manager: Option<std::rc::Rc<std::cell::RefCell<DownloadManager>>>,
    // TODO(plugin): plugin_manager is part of the plugin API
    plugin_manager: Option<Box<PluginManager>>,
    autoload_generator: Option<AutoloadGenerator>,
    archive_manager: Option<ArchiveManager>,
}

impl Composer {
    // TODO: change this information to Shirabe version.
    pub const VERSION: &'static str = "2.9.7";
    pub const BRANCH_ALIAS_VERSION: &'static str = "";
    pub const RELEASE_DATE: &'static str = "2026-04-14 13:31:52";
    pub const SOURCE_VERSION: &'static str = "";
    pub const RUNTIME_API_VERSION: &'static str = "2.2.2";

    pub fn new() -> Self {
        Self {
            inner: PartialComposer::default(),
            locker: None,
            download_manager: None,
            plugin_manager: None,
            autoload_generator: None,
            archive_manager: None,
        }
    }

    pub fn get_version() -> String {
        if Self::VERSION == "@package_version@" {
            return Self::SOURCE_VERSION.to_string();
        }
        if Self::BRANCH_ALIAS_VERSION != ""
            && Preg::is_match("{^[a-f0-9]{40}$}", Self::VERSION).unwrap_or(false)
        {
            return format!("{}+{}", Self::BRANCH_ALIAS_VERSION, Self::VERSION);
        }
        Self::VERSION.to_string()
    }

    pub fn set_locker(&mut self, locker: Locker) {
        self.locker = Some(locker);
    }

    pub fn get_locker(&self) -> &Locker {
        self.locker.as_ref().unwrap()
    }

    pub fn get_locker_mut(&mut self) -> &mut Locker {
        self.locker.as_mut().unwrap()
    }

    pub fn set_download_manager(
        &mut self,
        manager: std::rc::Rc<std::cell::RefCell<DownloadManager>>,
    ) {
        self.download_manager = Some(manager);
    }

    pub fn get_download_manager(&self) -> &std::rc::Rc<std::cell::RefCell<DownloadManager>> {
        self.download_manager.as_ref().unwrap()
    }

    pub fn set_archive_manager(&mut self, manager: ArchiveManager) {
        self.archive_manager = Some(manager);
    }

    pub fn get_archive_manager(&self) -> &ArchiveManager {
        self.archive_manager.as_ref().unwrap()
    }

    // TODO(plugin): set_plugin_manager is part of the plugin API
    pub fn set_plugin_manager(&mut self, manager: PluginManager) {
        self.plugin_manager = Some(Box::new(manager));
    }

    // TODO(plugin): get_plugin_manager is part of the plugin API
    pub fn get_plugin_manager(&self) -> &PluginManager {
        self.plugin_manager.as_ref().unwrap()
    }

    // TODO(plugin): get_plugin_manager_mut is part of the plugin API
    pub fn get_plugin_manager_mut(&mut self) -> &mut PluginManager {
        self.plugin_manager.as_mut().unwrap()
    }

    pub fn set_autoload_generator(&mut self, autoload_generator: AutoloadGenerator) {
        self.autoload_generator = Some(autoload_generator);
    }

    pub fn get_autoload_generator(&self) -> &AutoloadGenerator {
        self.autoload_generator.as_ref().unwrap()
    }

    pub fn get_autoload_generator_mut(&mut self) -> &mut AutoloadGenerator {
        self.autoload_generator.as_mut().unwrap()
    }

    pub fn get_package(&self) -> &dyn crate::package::RootPackageInterface {
        self.inner.get_package()
    }

    pub fn get_config(&self) -> &std::rc::Rc<std::cell::RefCell<crate::config::Config>> {
        self.inner.get_config()
    }

    pub fn get_config_mut(
        &mut self,
    ) -> &mut std::rc::Rc<std::cell::RefCell<crate::config::Config>> {
        self.inner.get_config_mut()
    }

    pub fn get_repository_manager(&self) -> &crate::repository::RepositoryManager {
        self.inner.get_repository_manager()
    }

    pub fn set_event_dispatcher(
        &mut self,
        dispatcher: std::rc::Rc<std::cell::RefCell<crate::event_dispatcher::EventDispatcher>>,
    ) {
        self.inner.set_event_dispatcher(dispatcher);
    }

    pub fn get_event_dispatcher(
        &self,
    ) -> &std::rc::Rc<std::cell::RefCell<crate::event_dispatcher::EventDispatcher>> {
        self.inner.get_event_dispatcher()
    }

    pub fn get_installation_manager(&self) -> &crate::installer::InstallationManager {
        self.inner.get_installation_manager()
    }

    pub fn get_installation_manager_mut(&mut self) -> &mut crate::installer::InstallationManager {
        self.inner.get_installation_manager_mut()
    }

    pub fn get_loop(&self) -> &std::rc::Rc<std::cell::RefCell<crate::util::r#loop::Loop>> {
        self.inner.get_loop()
    }

    pub fn set_loop(&mut self, r#loop: std::rc::Rc<std::cell::RefCell<crate::util::r#loop::Loop>>) {
        self.inner.set_loop(r#loop);
    }

    pub fn set_config(&mut self, config: std::rc::Rc<std::cell::RefCell<crate::config::Config>>) {
        self.inner.set_config(config);
    }

    pub fn set_global(&mut self) {
        self.inner.set_global();
    }

    pub fn set_repository_manager(&mut self, manager: crate::repository::RepositoryManager) {
        self.inner.set_repository_manager(manager);
    }

    pub fn set_installation_manager(&mut self, manager: crate::installer::InstallationManager) {
        self.inner.set_installation_manager(manager);
    }

    pub fn is_global(&self) -> bool {
        self.inner.is_global()
    }

    pub fn as_partial(&self) -> &crate::partial_composer::PartialComposer {
        &self.inner
    }

    pub fn set_package(&mut self, package: Box<dyn crate::package::RootPackageInterface>) {
        self.inner.set_package(package);
    }
}
