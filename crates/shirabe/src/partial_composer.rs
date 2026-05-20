//! ref: composer/src/Composer/PartialComposer.php

use crate::config::Config;
use crate::event_dispatcher::EventDispatcher;
use crate::installer::InstallationManager;
use crate::package::RootPackageInterface;
use crate::repository::RepositoryManager;
use crate::util::r#loop::Loop;

#[derive(Debug, Default)]
pub struct PartialComposer {
    global: bool,
    package: Option<Box<dyn RootPackageInterface>>,
    r#loop: Option<std::rc::Rc<std::cell::RefCell<Loop>>>,
    repository_manager: Option<RepositoryManager>,
    installation_manager: Option<InstallationManager>,
    config: Option<std::rc::Rc<std::cell::RefCell<Config>>>,
    event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
}

impl PartialComposer {
    pub fn set_package(&mut self, package: Box<dyn RootPackageInterface>) {
        self.package = Some(package);
    }

    pub fn get_package(&self) -> &dyn RootPackageInterface {
        self.package.as_deref().unwrap()
    }

    pub fn set_config(&mut self, config: std::rc::Rc<std::cell::RefCell<Config>>) {
        self.config = Some(config);
    }

    pub fn get_config(&self) -> &std::rc::Rc<std::cell::RefCell<Config>> {
        self.config.as_ref().unwrap()
    }

    pub fn get_config_mut(&mut self) -> &mut std::rc::Rc<std::cell::RefCell<Config>> {
        self.config.as_mut().unwrap()
    }

    pub fn set_loop(&mut self, r#loop: std::rc::Rc<std::cell::RefCell<Loop>>) {
        self.r#loop = Some(r#loop);
    }

    pub fn get_loop(&self) -> &std::rc::Rc<std::cell::RefCell<Loop>> {
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

    pub fn get_installation_manager_mut(&mut self) -> &mut InstallationManager {
        self.installation_manager.as_mut().unwrap()
    }

    pub fn set_event_dispatcher(
        &mut self,
        event_dispatcher: std::rc::Rc<std::cell::RefCell<EventDispatcher>>,
    ) {
        self.event_dispatcher = Some(event_dispatcher);
    }

    pub fn get_event_dispatcher(&self) -> &std::rc::Rc<std::cell::RefCell<EventDispatcher>> {
        self.event_dispatcher.as_ref().unwrap()
    }

    pub fn is_global(&self) -> bool {
        self.global
    }

    pub fn set_global(&mut self) {
        self.global = true;
    }

    /// TODO(phase-b): Emulates PHP `$composer instanceof Composer` check.
    /// PartialComposer cannot be a Composer here (Composer is a separate struct
    /// that wraps PartialComposer via composition), so this always returns false.
    pub fn is_full_composer(&self) -> bool {
        false
    }

    /// TODO(phase-b): Emulates PHP downcast to `Composer`.
    /// Returns self as `&dyn Any`; downcasting to Composer will always fail because
    /// PartialComposer is not a Composer in this Rust port.
    pub fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
