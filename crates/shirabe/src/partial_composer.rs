//! ref: composer/src/Composer/PartialComposer.php

use crate::config::Config;
use crate::event_dispatcher::event_dispatcher::EventDispatcher;
use crate::installer::installation_manager::InstallationManager;
use crate::package::root_package_interface::RootPackageInterface;
use crate::repository::repository_manager::RepositoryManager;
use crate::util::r#loop::Loop;

#[derive(Debug)]
pub struct PartialComposer {
    global: bool,
    package: Option<Box<dyn RootPackageInterface>>,
    r#loop: Option<Loop>,
    repository_manager: Option<RepositoryManager>,
    installation_manager: Option<InstallationManager>,
    config: Option<Config>,
    event_dispatcher: Option<EventDispatcher>,
}

impl PartialComposer {
    pub fn set_package(&mut self, package: Box<dyn RootPackageInterface>) {
        self.package = Some(package);
    }

    pub fn get_package(&self) -> &dyn RootPackageInterface {
        self.package.as_deref().unwrap()
    }

    pub fn set_config(&mut self, config: Config) {
        self.config = Some(config);
    }

    pub fn get_config(&self) -> &Config {
        self.config.as_ref().unwrap()
    }

    pub fn set_loop(&mut self, r#loop: Loop) {
        self.r#loop = Some(r#loop);
    }

    pub fn get_loop(&self) -> &Loop {
        self.r#loop.as_ref().unwrap()
    }

    pub fn set_repository_manager(&mut self, manager: RepositoryManager) {
        self.repository_manager = Some(manager);
    }

    pub fn get_repository_manager(&self) -> &RepositoryManager {
        self.repository_manager.as_ref().unwrap()
    }

    pub fn set_installation_manager(&mut self, manager: InstallationManager) {
        self.installation_manager = Some(manager);
    }

    pub fn get_installation_manager(&self) -> &InstallationManager {
        self.installation_manager.as_ref().unwrap()
    }

    pub fn set_event_dispatcher(&mut self, event_dispatcher: EventDispatcher) {
        self.event_dispatcher = Some(event_dispatcher);
    }

    pub fn get_event_dispatcher(&self) -> &EventDispatcher {
        self.event_dispatcher.as_ref().unwrap()
    }

    pub fn is_global(&self) -> bool {
        self.global
    }

    pub fn set_global(&mut self) {
        self.global = true;
    }
}
