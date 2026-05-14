//! ref: composer/src/Composer/Installer/PackageEvent.php

use indexmap::IndexMap;
use crate::composer::Composer;
use crate::dependency_resolver::operation::operation_interface::OperationInterface;
use crate::event_dispatcher::event::Event;
use crate::io::io_interface::IOInterface;
use crate::repository::repository_interface::RepositoryInterface;

#[derive(Debug)]
pub struct PackageEvent {
    inner: Event,
    composer: Composer,
    io: Box<dyn IOInterface>,
    dev_mode: bool,
    local_repo: Box<dyn RepositoryInterface>,
    operations: Vec<Box<dyn OperationInterface>>,
    operation: Box<dyn OperationInterface>,
}

impl PackageEvent {
    pub fn new(
        event_name: String,
        composer: Composer,
        io: Box<dyn IOInterface>,
        dev_mode: bool,
        local_repo: Box<dyn RepositoryInterface>,
        operations: Vec<Box<dyn OperationInterface>>,
        operation: Box<dyn OperationInterface>,
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

    pub fn get_composer(&self) -> &Composer {
        &self.composer
    }

    pub fn get_io(&self) -> &dyn IOInterface {
        self.io.as_ref()
    }

    pub fn is_dev_mode(&self) -> bool {
        self.dev_mode
    }

    pub fn get_local_repo(&self) -> &dyn RepositoryInterface {
        self.local_repo.as_ref()
    }

    pub fn get_operations(&self) -> &Vec<Box<dyn OperationInterface>> {
        &self.operations
    }

    pub fn get_operation(&self) -> &dyn OperationInterface {
        self.operation.as_ref()
    }
}
