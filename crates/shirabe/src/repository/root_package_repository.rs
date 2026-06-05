//! ref: composer/src/Composer/Repository/RootPackageRepository.php

use crate::package::BasePackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::package::RootPackageInterfaceHandle;
use crate::repository::ArrayRepository;
use crate::repository::RepositoryInterfaceWeakHandle;
use crate::repository::{ProviderInfo, RepositoryInterface, SearchResult};
use indexmap::IndexMap;

#[derive(Debug)]
pub struct RootPackageRepository {
    inner: ArrayRepository,
}

impl RootPackageRepository {
    pub fn new(package: RootPackageInterfaceHandle) -> Self {
        Self {
            inner: ArrayRepository::new(vec![package.into()]).expect("invalid root package"),
        }
    }

    pub fn get_repo_name(&self) -> String {
        "root package repo".to_string()
    }
}

impl shirabe_php_shim::Countable for RootPackageRepository {
    fn count(&self) -> i64 {
        self.inner.count()
    }
}

impl RepositoryInterface for RootPackageRepository {
    fn has_package(&self, package: PackageInterfaceHandle) -> bool {
        self.inner.has_package(package)
    }

    fn find_package(
        &mut self,
        name: &str,
        constraint: crate::repository::FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>> {
        self.inner.find_package(name, constraint)
    }

    fn find_packages(
        &mut self,
        name: &str,
        constraint: Option<crate::repository::FindPackageConstraint>,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        self.inner.find_packages(name, constraint)
    }

    fn get_packages(&mut self) -> anyhow::Result<Vec<BasePackageHandle>> {
        self.inner.get_packages()
    }

    fn load_packages(
        &mut self,
        package_name_map: IndexMap<String, Option<shirabe_semver::constraint::AnyConstraint>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
    ) -> anyhow::Result<crate::repository::LoadPackagesResult> {
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
        RootPackageRepository::get_repo_name(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_self_handle(&self, weak: RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }
}
