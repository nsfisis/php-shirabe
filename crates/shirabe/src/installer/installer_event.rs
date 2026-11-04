//! ref: composer/src/Composer/Installer/InstallerEvent.php

use crate::composer::ComposerWeakHandle;
use crate::dependency_resolver::AnyTransaction;
use crate::event_dispatcher::{Event, EventFlags};
use crate::io::IOInterface;
use bstr::BString;

#[derive(Debug)]
pub struct InstallerEvent {
    inner: Event,
    composer: ComposerWeakHandle,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    dev_mode: bool,
    execute_operations: bool,
    transaction: std::rc::Rc<AnyTransaction>,
}

impl InstallerEvent {
    pub fn new(
        event_name: impl Into<BString>,
        composer: ComposerWeakHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        dev_mode: bool,
        execute_operations: bool,
        transaction: std::rc::Rc<AnyTransaction>,
    ) -> Self {
        let inner = Event::new(event_name, vec![], EventFlags::default());
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

    pub fn get_transaction(&self) -> Option<&std::rc::Rc<AnyTransaction>> {
        Some(&self.transaction)
    }
}

crate::event_dispatcher::delegate_event_interface_to_inner!(InstallerEvent, inner);
