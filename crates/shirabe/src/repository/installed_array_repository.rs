//! ref: composer/src/Composer/Repository/InstalledArrayRepository.php

use crate::package::{BasePackageHandle, PackageInterfaceHandle};
use crate::repository::{
    AdvisoryProviderInterface, FindPackageConstraint, InstalledRepositoryInterface,
    LoadPackagesResult, ProviderInfo, RepositoryInterface, SearchResult, WritableArrayRepository,
    WritableRepositoryInterface,
};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::bconcat;
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug)]
pub struct InstalledArrayRepository {
    inner: WritableArrayRepository,
}

impl InstalledArrayRepository {
    pub fn new() -> anyhow::Result<Self> {
        Self::new_with_packages(Vec::new())
    }

    pub fn new_with_packages(packages: Vec<PackageInterfaceHandle>) -> anyhow::Result<Self> {
        Ok(Self {
            inner: WritableArrayRepository::new(packages)?,
        })
    }

    pub fn get_repo_name(&self) -> anyhow::Result<BString> {
        Ok(bconcat!("installed ", self.inner.get_repo_name()?))
    }
}

impl InstalledRepositoryInterface for InstalledArrayRepository {
    fn get_dev_mode(&self) -> Option<bool> {
        self.inner.get_dev_mode()
    }

    fn is_fresh(&self) -> bool {
        self.inner.base_count() == 0
    }
}

impl WritableRepositoryInterface for InstalledArrayRepository {
    fn write(
        &mut self,
        dev_mode: bool,
        installation_manager: &crate::installer::InstallationManager,
    ) -> anyhow::Result<()> {
        self.inner.write(dev_mode, installation_manager)
    }

    fn add_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<()> {
        self.inner.add_package(package)
    }

    fn remove_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<()> {
        self.inner.remove_package(package)
    }

    fn get_canonical_packages(&mut self) -> anyhow::Result<Vec<PackageInterfaceHandle>> {
        Ok(self.inner.get_canonical_packages())
    }

    fn reload(&mut self) -> anyhow::Result<()> {
        self.inner.reload();
        Ok(())
    }

    fn set_dev_package_names(&mut self, dev_package_names: Vec<BString>) {
        self.inner.set_dev_package_names(dev_package_names);
    }

    fn get_dev_package_names(&self) -> Vec<BString> {
        self.inner.get_dev_package_names()
    }
}

impl RepositoryInterface for InstalledArrayRepository {
    fn count(&mut self) -> anyhow::Result<usize> {
        self.inner.count()
    }

    fn has_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<bool> {
        self.inner.has_package(package)
    }
    fn find_package(
        &mut self,
        name: &BStr,
        constraint: FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>> {
        self.inner.find_package(name, constraint)
    }
    fn find_packages(
        &mut self,
        name: &BStr,
        constraint: Option<FindPackageConstraint>,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        self.inner.find_packages(name, constraint)
    }
    fn get_packages(&mut self) -> anyhow::Result<Vec<BasePackageHandle>> {
        self.inner.get_packages()
    }
    fn load_packages(
        &mut self,
        package_name_map: IndexMap<BString, Option<AnyConstraint>>,
        acceptable_stabilities: IndexMap<BString, i64>,
        stability_flags: IndexMap<BString, i64>,
        already_loaded: IndexMap<BString, IndexMap<BString, PackageInterfaceHandle>>,
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
        query: BString,
        mode: i64,
        r#type: Option<BString>,
    ) -> anyhow::Result<Vec<SearchResult>> {
        self.inner.search(query, mode, r#type)
    }
    fn get_providers(
        &mut self,
        package_name: BString,
    ) -> anyhow::Result<IndexMap<BString, ProviderInfo>> {
        self.inner.get_providers(package_name)
    }
    fn get_repo_name(&self) -> anyhow::Result<BString> {
        Ok(bconcat!("installed ", self.inner.get_repo_name()?))
    }
    fn as_advisory_provider(&self) -> Option<&dyn AdvisoryProviderInterface> {
        None
    }
    fn as_installed_repository_interface(
        &self,
    ) -> Option<&(dyn InstalledRepositoryInterface + 'static)> {
        Some(self)
    }
    fn as_installed_repository_interface_mut(
        &mut self,
    ) -> Option<&mut (dyn InstalledRepositoryInterface + 'static)> {
        Some(self)
    }
    fn as_writable_repository_interface_mut(
        &mut self,
    ) -> Option<&mut dyn crate::repository::WritableRepositoryInterface> {
        Some(self)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn set_self_handle(&self, weak: crate::repository::RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }
}
