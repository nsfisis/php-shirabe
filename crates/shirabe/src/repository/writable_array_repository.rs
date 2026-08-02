//! ref: composer/src/Composer/Repository/WritableArrayRepository.php

use crate::installer::InstallationManager;
use crate::package::BasePackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::repository::ArrayRepository;
use crate::repository::RepositoryInterface;
use crate::repository::RepositoryInterfaceWeakHandle;
use crate::repository::{FindPackageConstraint, LoadPackagesResult, ProviderInfo, SearchResult};
use indexmap::IndexMap;
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug)]
pub struct WritableArrayRepository {
    inner: ArrayRepository,
    // RefCell so that FilesystemRepository::initialize can stay `&self` (late-bound
    // initialization out of shared contexts such as getRepoName).
    pub(crate) dev_package_names: std::cell::RefCell<Vec<String>>,
    dev_mode: Option<bool>,
}

impl WritableArrayRepository {
    pub fn new(packages: Vec<crate::package::PackageInterfaceHandle>) -> anyhow::Result<Self> {
        Ok(Self {
            inner: ArrayRepository::new(packages)?,
            dev_package_names: std::cell::RefCell::new(Vec::new()),
            dev_mode: None,
        })
    }

    /// Returns true if dev requirements were installed, false if --no-dev was used, None if yet unknown.
    pub fn get_dev_mode(&self) -> Option<bool> {
        self.dev_mode
    }

    /// See `ArrayRepository::base_count`; kept on `&self` for `is_fresh` callers.
    pub(crate) fn base_count(&self) -> usize {
        self.inner.base_count()
    }

    pub fn set_dev_package_names(&self, dev_package_names: Vec<String>) {
        *self.dev_package_names.borrow_mut() = dev_package_names;
    }

    pub fn get_dev_package_names(&self) -> Vec<String> {
        self.dev_package_names.borrow().clone()
    }

    pub fn write(
        &mut self,
        dev_mode: bool,
        _installation_manager: &InstallationManager,
    ) -> anyhow::Result<()> {
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

    pub fn add_package(
        &self,
        package: crate::package::PackageInterfaceHandle,
    ) -> anyhow::Result<()> {
        self.inner.add_package(package)
    }

    pub fn set_self_handle(&self, weak: RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }

    pub fn remove_package(
        &mut self,
        package: crate::package::PackageInterfaceHandle,
    ) -> anyhow::Result<()> {
        self.inner.remove_package(package);
        Ok(())
    }

    pub fn initialize(&self) -> anyhow::Result<()> {
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

    pub fn get_repo_name(&self) -> anyhow::Result<String> {
        RepositoryInterface::get_repo_name(&self.inner)
    }
}

impl RepositoryInterface for WritableArrayRepository {
    fn count(&mut self) -> anyhow::Result<usize> {
        self.inner.count()
    }

    fn has_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<bool> {
        self.inner.has_package(package)
    }

    fn find_package(
        &mut self,
        name: &str,
        constraint: FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>> {
        self.inner.find_package(name, constraint)
    }

    fn find_packages(
        &mut self,
        name: &str,
        constraint: Option<FindPackageConstraint>,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        self.inner.find_packages(name, constraint)
    }

    fn get_packages(&mut self) -> anyhow::Result<Vec<BasePackageHandle>> {
        self.inner.get_packages()
    }

    fn load_packages(
        &mut self,
        package_name_map: IndexMap<String, Option<AnyConstraint>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
    ) -> anyhow::Result<LoadPackagesResult> {
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
    ) -> anyhow::Result<Vec<SearchResult>> {
        self.inner.search(query, mode, r#type)
    }

    fn get_providers(
        &mut self,
        package_name: String,
    ) -> anyhow::Result<IndexMap<String, ProviderInfo>> {
        self.inner.get_providers(package_name)
    }

    fn get_repo_name(&self) -> anyhow::Result<String> {
        self.inner.get_repo_name()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_self_handle(&self, weak: RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }
}
