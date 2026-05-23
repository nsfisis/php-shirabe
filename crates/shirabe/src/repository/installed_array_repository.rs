//! ref: composer/src/Composer/Repository/InstalledArrayRepository.php

use indexmap::IndexMap;
use shirabe_php_shim::Countable;
use shirabe_semver::constraint::AnyConstraint;

use crate::package::BasePackage;
use crate::package::PackageInterface;
use crate::repository::AdvisoryProviderInterface;
use crate::repository::InstalledRepositoryInterface;
use crate::repository::WritableArrayRepository;
use crate::repository::WritableRepositoryInterface;
use crate::repository::{
    FindPackageConstraint, LoadPackagesResult, ProviderInfo, RepositoryInterface, SearchResult,
};

#[derive(Debug)]
pub struct InstalledArrayRepository {
    inner: WritableArrayRepository,
}

impl InstalledArrayRepository {
    pub fn new() -> anyhow::Result<Self> {
        Self::new_with_packages(Vec::new())
    }

    pub fn new_with_packages(packages: Vec<Box<dyn PackageInterface>>) -> anyhow::Result<Self> {
        Ok(Self {
            inner: WritableArrayRepository::new(packages)?,
        })
    }

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

impl WritableRepositoryInterface for InstalledArrayRepository {
    fn write(
        &mut self,
        dev_mode: bool,
        installation_manager: &crate::installer::InstallationManager,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn add_package(
        &mut self,
        package: Box<dyn crate::package::PackageInterface>,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn remove_package(
        &mut self,
        package: &dyn crate::package::PackageInterface,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn get_canonical_packages(&self) -> Vec<Box<dyn crate::package::PackageInterface>> {
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

impl Countable for InstalledArrayRepository {
    fn count(&self) -> i64 {
        todo!()
    }
}

impl RepositoryInterface for InstalledArrayRepository {
    fn has_package(&self, _package: &dyn PackageInterface) -> bool {
        todo!()
    }
    fn find_package(
        &self,
        _name: &str,
        _constraint: FindPackageConstraint,
    ) -> Option<Box<dyn BasePackage>> {
        todo!()
    }
    fn find_packages(
        &self,
        _name: &str,
        _constraint: Option<FindPackageConstraint>,
    ) -> Vec<Box<dyn BasePackage>> {
        todo!()
    }
    fn get_packages(&self) -> Vec<Box<dyn BasePackage>> {
        todo!()
    }
    fn load_packages(
        &self,
        _package_name_map: IndexMap<String, Option<AnyConstraint>>,
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
