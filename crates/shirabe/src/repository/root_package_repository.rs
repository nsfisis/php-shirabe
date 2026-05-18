//! ref: composer/src/Composer/Repository/RootPackageRepository.php

use crate::package::base_package::BasePackage;
use crate::package::package_interface::PackageInterface;
use crate::package::root_package_interface::RootPackageInterface;
use crate::repository::array_repository::ArrayRepository;
use crate::repository::repository_interface::{ProviderInfo, RepositoryInterface, SearchResult};
use indexmap::IndexMap;

#[derive(Debug)]
pub struct RootPackageRepository {
    inner: ArrayRepository,
}

impl RootPackageRepository {
    pub fn new(package: Box<dyn RootPackageInterface>) -> Self {
        Self {
            // TODO(phase-b): RootPackageInterface vs BasePackage upcast + ArrayRepository::new error
            inner: ArrayRepository::new(vec![todo!(
                "convert Box<dyn RootPackageInterface> to Box<dyn BasePackage>"
            )])
            .expect("invalid root package"),
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
    fn has_package(&self, package: &dyn PackageInterface) -> bool {
        self.inner.has_package(package)
    }

    fn find_package(
        &self,
        name: String,
        constraint: crate::repository::repository_interface::FindPackageConstraint,
    ) -> Option<Box<dyn BasePackage>> {
        self.inner.find_package(name, constraint)
    }

    fn find_packages(
        &self,
        name: String,
        constraint: Option<crate::repository::repository_interface::FindPackageConstraint>,
    ) -> Vec<Box<dyn BasePackage>> {
        self.inner.find_packages(name, constraint)
    }

    fn get_packages(&self) -> Vec<Box<dyn BasePackage>> {
        self.inner.get_packages()
    }

    fn load_packages(
        &self,
        package_name_map: IndexMap<
            String,
            Option<Box<dyn shirabe_semver::constraint::constraint_interface::ConstraintInterface>>,
        >,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, Box<dyn PackageInterface>>>,
    ) -> crate::repository::repository_interface::LoadPackagesResult {
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
        RootPackageRepository::get_repo_name(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
