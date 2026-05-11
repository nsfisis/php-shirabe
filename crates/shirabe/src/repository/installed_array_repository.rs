//! ref: composer/src/Composer/Repository/InstalledArrayRepository.php

use crate::repository::installed_repository_interface::InstalledRepositoryInterface;
use crate::repository::writable_array_repository::WritableArrayRepository;

#[derive(Debug)]
pub struct InstalledArrayRepository {
    inner: WritableArrayRepository,
}

impl InstalledArrayRepository {
    pub fn get_repo_name(&self) -> String {
        format!("installed {}", self.inner.get_repo_name())
    }
}

impl InstalledRepositoryInterface for InstalledArrayRepository {
    fn get_dev_mode(&self) -> Option<bool> {
        self.inner.get_dev_mode()
    }

    fn is_fresh(&self) -> bool {
        self.inner.count() == 0
    }
}
