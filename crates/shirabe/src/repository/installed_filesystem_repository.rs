//! ref: composer/src/Composer/Repository/InstalledFilesystemRepository.php

use crate::repository::filesystem_repository::FilesystemRepository;
use crate::repository::installed_repository_interface::InstalledRepositoryInterface;

#[derive(Debug)]
pub struct InstalledFilesystemRepository {
    inner: FilesystemRepository,
}

impl InstalledFilesystemRepository {
    pub fn get_repo_name(&self) -> String {
        format!("installed {}", self.inner.get_repo_name())
    }
}

impl InstalledRepositoryInterface for InstalledFilesystemRepository {
    fn get_dev_mode(&self) -> Option<bool> {
        self.inner.get_dev_mode()
    }

    fn is_fresh(&self) -> bool {
        !self.inner.file.exists()
    }
}
