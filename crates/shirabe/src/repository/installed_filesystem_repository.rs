//! ref: composer/src/Composer/Repository/InstalledFilesystemRepository.php

use indexmap::IndexMap;
use shirabe_php_shim::Countable;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;

use crate::package::base_package::BasePackage;
use crate::package::package_interface::PackageInterface;
use crate::repository::advisory_provider_interface::AdvisoryProviderInterface;
use crate::repository::filesystem_repository::FilesystemRepository;
use crate::repository::installed_repository_interface::InstalledRepositoryInterface;
use crate::repository::repository_interface::{
    FindPackageConstraint, LoadPackagesResult, ProviderInfo, RepositoryInterface, SearchResult,
};
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

impl Countable for InstalledFilesystemRepository {
    fn count(&self) -> i64 {
        todo!()
    }
}

impl RepositoryInterface for InstalledFilesystemRepository {
    fn has_package(&self, _package: &dyn PackageInterface) -> bool {
        todo!()
    }
    fn find_package(
        &self,
        _name: String,
        _constraint: FindPackageConstraint,
    ) -> Option<Box<dyn BasePackage>> {
        todo!()
    }
    fn find_packages(
        &self,
        _name: String,
        _constraint: Option<FindPackageConstraint>,
    ) -> Vec<Box<dyn BasePackage>> {
        todo!()
    }
    fn get_packages(&self) -> Vec<Box<dyn BasePackage>> {
        todo!()
    }
    fn load_packages(
        &self,
        _package_name_map: IndexMap<String, Option<Box<dyn ConstraintInterface>>>,
        _acceptable_stabilities: IndexMap<String, i64>,
        _stability_flags: IndexMap<String, i64>,
        _already_loaded: IndexMap<String, IndexMap<String, Box<dyn PackageInterface>>>,
    ) -> LoadPackagesResult {
        todo!()
    }
    fn search(&self, _query: String, _mode: i64, _type: Option<String>) -> Vec<SearchResult> {
        todo!()
    }
    fn get_providers(&self, _package_name: String) -> IndexMap<String, ProviderInfo> {
        todo!()
    }
    fn get_repo_name(&self) -> String {
        todo!()
    }
    fn as_advisory_provider(&self) -> Option<&dyn AdvisoryProviderInterface> {
        None
    }
    fn as_any(&self) -> &dyn std::any::Any {
        todo!()
    }
}
