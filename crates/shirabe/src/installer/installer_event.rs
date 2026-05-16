//! ref: composer/src/Composer/Installer/InstallerEvent.php

use crate::composer::Composer;
use crate::dependency_resolver::transaction::Transaction;
use crate::event_dispatcher::event::Event;
use crate::io::io_interface::IOInterface;

#[derive(Debug)]
pub struct InstallerEvent {
    inner: Event,
    composer: Composer,
    io: Box<dyn IOInterface>,
    dev_mode: bool,
    execute_operations: bool,
    transaction: Transaction,
}

impl InstallerEvent {
    pub fn new(
        event_name: String,
        composer: Composer,
        io: Box<dyn IOInterface>,
        dev_mode: bool,
        execute_operations: bool,
        transaction: Transaction,
    ) -> Self {
        let inner = Event::new(event_name, vec![], vec![]);
        Self {
            inner,
            composer,
            io,
            dev_mode,
            execute_operations,
            transaction,
        }
    }

    pub fn get_composer(&self) -> &Composer {
        &self.composer
    }

    pub fn get_io(&self) -> &dyn IOInterface {
        self.io.as_ref()
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
