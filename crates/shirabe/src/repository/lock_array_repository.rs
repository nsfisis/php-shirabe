//! ref: composer/src/Composer/Repository/LockArrayRepository.php

use crate::package::BasePackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::repository::ArrayRepository;
use crate::repository::CanonicalPackagesTrait;
use crate::repository::{
    FindPackageConstraint, LoadPackagesResult, ProviderInfo, RepositoryInterface,
    RepositoryInterfaceWeakHandle, SearchResult,
};
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
        RepositoryInterface::get_packages(&mut self.inner)
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

    fn get_repo_name(&self) -> String {
        "lock repo".to_string()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_self_handle(&self, weak: RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }
}
