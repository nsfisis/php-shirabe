//! ref: composer/src/Composer/Repository/WritableArrayRepository.php

use crate::installer::InstallationManager;
use crate::package::BasePackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::repository::ArrayRepository;
use crate::repository::RepositoryInterface;
use crate::repository::RepositoryInterfaceWeakHandle;
use crate::repository::{FindPackageConstraint, LoadPackagesResult, ProviderInfo, SearchResult};
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug)]
pub struct WritableArrayRepository {
    inner: ArrayRepository,
    pub(crate) dev_package_names: Vec<String>,
    dev_mode: Option<bool>,
}

impl WritableArrayRepository {
    pub fn new(packages: Vec<crate::package::PackageInterfaceHandle>) -> Result<Self> {
        Ok(Self {
            inner: ArrayRepository::new(packages)?,
            dev_package_names: Vec::new(),
            dev_mode: None,
        })
    }

    /// Returns true if dev requirements were installed, false if --no-dev was used, None if yet unknown.
    pub fn get_dev_mode(&self) -> Option<bool> {
        self.dev_mode
    }

    pub fn set_dev_package_names(&mut self, dev_package_names: Vec<String>) {
        self.dev_package_names = dev_package_names;
    }

    pub fn get_dev_package_names(&self) -> &Vec<String> {
        &self.dev_package_names
    }

    pub fn write(
        &mut self,
        dev_mode: bool,
        _installation_manager: &InstallationManager,
    ) -> Result<()> {
        self.dev_mode = Some(dev_mode);
        Ok(())
    }

    pub fn reload(&mut self) {
        self.dev_mode = None;
    }

    pub fn reset_packages(&self) {
        self.inner.reset_packages();
    }

    pub(crate) fn is_initialized(&self) -> bool {
        self.inner.is_initialized()
    }

    pub fn add_package(&mut self, package: crate::package::PackageInterfaceHandle) -> Result<()> {
        self.inner.add_package(package)
    }

    pub fn set_self_handle(&self, weak: RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }

    pub fn remove_package(
        &mut self,
        package: crate::package::PackageInterfaceHandle,
    ) -> Result<()> {
        self.inner.remove_package(package);
        Ok(())
    }

    pub fn initialize(&mut self) -> Result<()> {
        self.inner.initialize();
        Ok(())
    }

    /// Get unique packages (at most one package of each name), with aliases resolved and removed.
    pub fn get_canonical_packages(&self) -> Vec<crate::package::PackageInterfaceHandle> {
        let packages = self.inner.get_packages_internal();

        // get at most one package of each name, preferring non-aliased ones
        let mut packages_by_name: IndexMap<String, crate::package::PackageInterfaceHandle> =
            IndexMap::new();
        for package in packages {
            let name = package.get_name();
            let prefer_replace = packages_by_name
                .get(&name)
                .map(|existing| existing.as_alias().is_some())
                .unwrap_or(true);
            if prefer_replace {
                packages_by_name.insert(name, package);
            }
        }

        // unfold aliased packages
        let mut canonical_packages = Vec::new();
        for mut package in packages_by_name.into_values() {
            while let Some(alias) = package.as_alias() {
                package = alias.get_alias_of().into();
            }
            canonical_packages.push(package);
        }

        canonical_packages
    }

    pub fn get_packages(&mut self) -> anyhow::Result<Vec<crate::package::BasePackageHandle>> {
        self.inner.get_packages()
    }

    pub fn get_repo_name(&self) -> String {
        self.inner.get_repo_name()
    }
}

impl RepositoryInterface for WritableArrayRepository {
    fn count(&self) -> anyhow::Result<usize> {
        self.inner.count()
    }

    fn has_package(&self, package: PackageInterfaceHandle) -> bool {
        self.inner.has_package(package)
    }

    fn find_package(
        &mut self,
        name: &str,
        constraint: FindPackageConstraint,
    ) -> Result<Option<BasePackageHandle>> {
        self.inner.find_package(name, constraint)
    }

    fn find_packages(
        &mut self,
        name: &str,
        constraint: Option<FindPackageConstraint>,
    ) -> Result<Vec<BasePackageHandle>> {
        self.inner.find_packages(name, constraint)
    }

    fn get_packages(&mut self) -> Result<Vec<BasePackageHandle>> {
        self.inner.get_packages()
    }

    fn load_packages(
        &mut self,
        package_name_map: IndexMap<String, Option<AnyConstraint>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
    ) -> Result<LoadPackagesResult> {
        self.inner.load_packages(
            package_name_map,
            acceptable_stabilities,
            stability_flags,
            already_loaded,
        )
    }

    fn search(
        &mut self,
        query: String,
        mode: i64,
        r#type: Option<String>,
    ) -> Result<Vec<SearchResult>> {
        self.inner.search(query, mode, r#type)
    }

    fn get_providers(&mut self, package_name: String) -> Result<IndexMap<String, ProviderInfo>> {
        self.inner.get_providers(package_name)
    }

    fn get_repo_name(&self) -> String {
        self.inner.get_repo_name()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_self_handle(&self, weak: RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }
}
