//! ref: composer/src/Composer/DependencyResolver/LocalRepoTransaction.php

use super::Transaction;
use crate::dependency_resolver::operation::AnyOperation;
use crate::repository::{InstalledRepositoryInterface, RepositoryInterface};

#[derive(Debug)]
pub struct LocalRepoTransaction {
    inner: Transaction,
}

impl LocalRepoTransaction {
    pub fn new(
        locked_repository: &mut dyn RepositoryInterface,
        local_repository: &mut dyn InstalledRepositoryInterface,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            inner: Transaction::new(
                local_repository.get_packages()?,
                locked_repository.get_packages()?,
            ),
        })
    }

    pub fn get_operations(&self) -> &Vec<std::rc::Rc<AnyOperation>> {
        self.inner.get_operations()
    }
}
