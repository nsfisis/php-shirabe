//! ref: composer/src/Composer/Installer/InstallerEvent.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use crate::composer::ComposerWeakHandle;
use crate::dependency_resolver::Transaction;
use crate::event_dispatcher::Event;
use crate::event_dispatcher::EventInterface;
use crate::io::IOInterface;

#[derive(Debug)]
pub struct InstallerEvent {
    inner: Event,
    composer: ComposerWeakHandle,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    dev_mode: bool,
    execute_operations: bool,
    transaction: Transaction,
}

impl InstallerEvent {
    pub fn new(
        event_name: String,
        composer: ComposerWeakHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        dev_mode: bool,
        execute_operations: bool,
        transaction: Transaction,
    ) -> Self {
        let inner = Event::new(event_name, vec![], indexmap::IndexMap::new());
        Self {
            inner,
            composer,
            io,
            dev_mode,
            execute_operations,
            transaction,
        }
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

    pub fn is_executing_operations(&self) -> bool {
        self.execute_operations
    }

    pub fn get_transaction(&self) -> Option<&Transaction> {
        Some(&self.transaction)
    }
}

impl EventInterface for InstallerEvent {
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
