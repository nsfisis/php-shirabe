//! ref: composer/src/Composer/Composer.php

use shirabe_external_packages::composer::pcre::preg::Preg;

use crate::autoload::autoload_generator::AutoloadGenerator;
use crate::downloader::download_manager::DownloadManager;
use crate::package::archiver::archive_manager::ArchiveManager;
use crate::package::locker::Locker;
use crate::partial_composer::PartialComposer;
use crate::plugin::plugin_manager::PluginManager;

#[derive(Debug)]
pub struct Composer {
    inner: PartialComposer,
    locker: Option<Locker>,
    download_manager: Option<DownloadManager>,
    // TODO(plugin): plugin_manager is part of the plugin API
    plugin_manager: Option<PluginManager>,
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

    pub fn get_version() -> String {
        if Self::VERSION == "@package_version@" {
            return Self::SOURCE_VERSION.to_string();
        }
        if Self::BRANCH_ALIAS_VERSION != "" && Preg::is_match("{^[a-f0-9]{40}$}", Self::VERSION).unwrap_or(false) {
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

    pub fn set_download_manager(&mut self, manager: DownloadManager) {
        self.download_manager = Some(manager);
    }

    pub fn get_download_manager(&self) -> &DownloadManager {
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
        self.plugin_manager = Some(manager);
    }

    // TODO(plugin): get_plugin_manager is part of the plugin API
    pub fn get_plugin_manager(&self) -> &PluginManager {
        self.plugin_manager.as_ref().unwrap()
    }

    pub fn set_autoload_generator(&mut self, autoload_generator: AutoloadGenerator) {
        self.autoload_generator = Some(autoload_generator);
    }

    pub fn get_autoload_generator(&self) -> &AutoloadGenerator {
        self.autoload_generator.as_ref().unwrap()
    }
}
