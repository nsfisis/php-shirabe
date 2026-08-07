//! ref: composer/src/Composer/Installer/PackageEvent.php

use crate::composer::ComposerWeakHandle;
use crate::dependency_resolver::operation::AnyOperation;
use crate::event_dispatcher::Event;
use crate::event_dispatcher::EventInterface;
use crate::io::IOInterface;
use crate::repository::InstalledRepositoryInterfaceHandle;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// The operations are shared rather than owned so that the same operation crosses the plugin
/// boundary as one object for both the pre- and the post-event, as it does in PHP.
#[derive(Debug)]
pub struct PackageEvent {
    inner: Event,
    composer: ComposerWeakHandle,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    dev_mode: bool,
    local_repo: InstalledRepositoryInterfaceHandle,
    operations: Vec<std::rc::Rc<AnyOperation>>,
    operation: std::rc::Rc<AnyOperation>,
}

impl PackageEvent {
    pub fn new(
        event_name: String,
        composer: ComposerWeakHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        dev_mode: bool,
        local_repo: InstalledRepositoryInterfaceHandle,
        operations: Vec<std::rc::Rc<AnyOperation>>,
        operation: std::rc::Rc<AnyOperation>,
    ) -> Self {
        Self {
            inner: Event::new(event_name, vec![], IndexMap::new()),
            composer,
            io,
            dev_mode,
            local_repo,
            operations,
            operation,
        }
    }

    pub fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    pub fn get_composer(&self) -> &ComposerWeakHandle {
        &self.composer
    }

    pub fn get_io(&self) -> std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
        self.io.clone()
    }

    pub fn is_dev_mode(&self) -> bool {
        self.dev_mode
    }

    pub fn get_local_repo(&self) -> InstalledRepositoryInterfaceHandle {
        self.local_repo.clone()
    }

    pub fn get_operations(&self) -> &Vec<std::rc::Rc<AnyOperation>> {
        &self.operations
    }

    pub fn get_operation(&self) -> &std::rc::Rc<AnyOperation> {
        &self.operation
    }
}

impl EventInterface for PackageEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    fn get_arguments(&self) -> &Vec<String> {
        self.inner.get_arguments()
    }

    fn get_flags(&self) -> &IndexMap<String, PhpMixed> {
        self.inner.get_flags()
    }

    fn is_propagation_stopped(&self) -> bool {
        self.inner.is_propagation_stopped()
    }

    fn stop_propagation(&mut self) {
        self.inner.stop_propagation();
    }
}
