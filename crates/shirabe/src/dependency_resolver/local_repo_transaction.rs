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
        Self {
            inner: Transaction::new(
                local_repository.get_packages(),
                locked_repository.get_packages(),
            ),
        }
    }

    pub fn get_operations(
        &self,
    ) -> Vec<std::rc::Rc<dyn crate::dependency_resolver::operation::OperationInterface>> {
        // TODO(phase-b): delegate to inner transaction once operations are typed.
        Vec::new()
    }
}
