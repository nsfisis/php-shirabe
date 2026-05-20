//! ref: composer/src/Composer/DependencyResolver/LocalRepoTransaction.php

use super::Transaction;
use crate::repository::InstalledRepositoryInterface;
use crate::repository::RepositoryInterface;

#[derive(Debug)]
pub struct LocalRepoTransaction {
    inner: Transaction,
}

impl LocalRepoTransaction {
    pub fn new(
        locked_repository: &dyn RepositoryInterface,
        local_repository: &dyn InstalledRepositoryInterface,
    ) -> Self {
        // TODO(phase-b): RepositoryInterface::get_packages returns Box<dyn BasePackage>
        // but Transaction::new wants Box<dyn PackageInterface>. Upcast each via PackageInterface
        // trait once a `into_package_interface` helper is added.
        let _ = (locked_repository, local_repository);
        Self {
            inner: Transaction::new(Vec::new(), Vec::new()),
        }
    }

    pub fn get_operations(
        &self,
    ) -> Vec<Box<dyn crate::dependency_resolver::operation::OperationInterface>> {
        // TODO(phase-b): delegate to inner transaction once operations are typed.
        Vec::new()
    }
}
