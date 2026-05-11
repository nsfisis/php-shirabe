//! ref: composer/src/Composer/DependencyResolver/LocalRepoTransaction.php

use crate::repository::installed_repository_interface::InstalledRepositoryInterface;
use crate::repository::repository_interface::RepositoryInterface;
use super::transaction::Transaction;

#[derive(Debug)]
pub struct LocalRepoTransaction {
    inner: Transaction,
}

impl LocalRepoTransaction {
    pub fn new(locked_repository: &dyn RepositoryInterface, local_repository: &dyn InstalledRepositoryInterface) -> Self {
        Self {
            inner: Transaction::new(
                local_repository.get_packages(),
                locked_repository.get_packages(),
            ),
        }
    }
}
