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
use shirabe_php_shim::Countable;
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug)]
pub struct LockArrayRepository {
    inner: ArrayRepository,
}

impl CanonicalPackagesTrait for LockArrayRepository {
    fn get_packages(&self) -> Vec<PackageInterfaceHandle> {
        todo!()
    }
}

impl LockArrayRepository {
    pub fn clone_box(&self) -> Box<dyn RepositoryInterface> {
        todo!()
    }
}

impl Countable for LockArrayRepository {
    fn count(&self) -> i64 {
        self.inner.count()
    }
}

impl RepositoryInterface for LockArrayRepository {
    fn has_package(&self, package: PackageInterfaceHandle) -> bool {
        self.inner.has_package(package)
    }

    fn find_package(
        &self,
        name: &str,
        constraint: FindPackageConstraint,
    ) -> Option<BasePackageHandle> {
        self.inner.find_package(name, constraint)
    }

    fn find_packages(
        &self,
        name: &str,
        constraint: Option<FindPackageConstraint>,
    ) -> Vec<BasePackageHandle> {
        self.inner.find_packages(name, constraint)
    }

    fn get_packages(&self) -> Vec<BasePackageHandle> {
        RepositoryInterface::get_packages(&self.inner)
    }

    fn load_packages(
        &self,
        package_name_map: IndexMap<String, Option<AnyConstraint>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
    ) -> LoadPackagesResult {
        self.inner.load_packages(
            package_name_map,
            acceptable_stabilities,
            stability_flags,
            already_loaded,
        )
    }

    fn search(&self, query: String, mode: i64, r#type: Option<String>) -> Vec<SearchResult> {
        self.inner.search(query, mode, r#type)
    }

    fn get_providers(&self, package_name: String) -> IndexMap<String, ProviderInfo> {
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
