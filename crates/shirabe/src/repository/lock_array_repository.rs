//! ref: composer/src/Composer/Repository/LockArrayRepository.php

use crate::package::{BasePackageHandle, PackageInterfaceHandle};
use crate::repository::{
    ArrayRepository, CanonicalPackagesTrait, FindPackageConstraint, LoadPackagesResult,
    ProviderInfo, RepositoryInterface, RepositoryInterfaceWeakHandle, SearchResult,
};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug)]
pub struct LockArrayRepository {
    inner: ArrayRepository,
}

impl CanonicalPackagesTrait for LockArrayRepository {}

impl LockArrayRepository {
    pub fn new(packages: Vec<PackageInterfaceHandle>) -> anyhow::Result<Self> {
        Ok(Self {
            inner: ArrayRepository::new(packages)?,
        })
    }

    pub fn add_package(&self, package: PackageInterfaceHandle) -> anyhow::Result<()> {
        self.inner.add_package(package)
    }
}

impl RepositoryInterface for LockArrayRepository {
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
        RepositoryInterface::get_packages(&mut self.inner)
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
        Ok(bstr::BString::from("lock repo".to_string()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_self_handle(&self, weak: RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }
}
