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
        // TODO(phase-c): RepositoryInterface::get_packages yields BasePackageHandle; widen each to
        // PackageInterfaceHandle (via .into()) and feed them to Transaction::new once the repository
        // getters expose handles here.
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
