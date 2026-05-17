//! ref: composer/src/Composer/Repository/InstalledFilesystemRepository.php

use crate::repository::filesystem_repository::FilesystemRepository;
use crate::repository::installed_repository_interface::InstalledRepositoryInterface;
use crate::repository::writable_repository_interface::WritableRepositoryInterface;

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

impl WritableRepositoryInterface for InstalledFilesystemRepository {
    fn write(
        &mut self,
        dev_mode: bool,
        installation_manager: &crate::installer::installation_manager::InstallationManager,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn add_package(
        &mut self,
        package: Box<dyn crate::package::package_interface::PackageInterface>,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn remove_package(
        &mut self,
        package: &dyn crate::package::package_interface::PackageInterface,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn get_canonical_packages(
        &self,
    ) -> Vec<Box<dyn crate::package::package_interface::PackageInterface>> {
        todo!()
    }

    fn reload(&mut self) {
        todo!()
    }

    fn set_dev_package_names(&mut self, dev_package_names: Vec<String>) {
        todo!()
    }

    fn get_dev_package_names(&self) -> &Vec<String> {
        todo!()
    }
}
