//! ref: composer/src/Composer/Repository/RepositoryInterface.php

use crate::package::base_package::BasePackage;
use crate::package::package_interface::PackageInterface;
use crate::repository::advisory_provider_interface::AdvisoryProviderInterface;
use indexmap::IndexMap;
use shirabe_php_shim::Countable;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;

pub enum FindPackageConstraint {
    String(String),
    Constraint(Box<dyn ConstraintInterface>),
}

pub struct LoadPackagesResult {
    pub names_found: Vec<String>,
    pub packages: Vec<Box<BasePackage>>,
}

pub enum AbandonedInfo {
    Replacement(String),
    Abandoned,
}

pub struct SearchResult {
    pub name: String,
    pub description: Option<String>,
    pub abandoned: Option<AbandonedInfo>,
    pub url: Option<String>,
}

pub struct ProviderInfo {
    pub name: String,
    pub description: Option<String>,
    pub r#type: String,
}

pub const SEARCH_FULLTEXT: i64 = 0;
pub const SEARCH_NAME: i64 = 1;
pub const SEARCH_VENDOR: i64 = 2;

pub trait RepositoryInterface: Countable {
    fn has_package(&self, package: &dyn PackageInterface) -> bool;

    fn find_package(
        &self,
        name: String,
        constraint: FindPackageConstraint,
    ) -> Option<Box<BasePackage>>;

    fn find_packages(
        &self,
        name: String,
        constraint: Option<FindPackageConstraint>,
    ) -> Vec<Box<BasePackage>>;

    fn get_packages(&self) -> Vec<Box<BasePackage>>;

    fn load_packages(
        &self,
        package_name_map: IndexMap<String, Option<Box<dyn ConstraintInterface>>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, Box<dyn PackageInterface>>>,
    ) -> LoadPackagesResult;

    fn search(&self, query: String, mode: i64, r#type: Option<String>) -> Vec<SearchResult>;

    fn get_providers(&self, package_name: String) -> IndexMap<String, ProviderInfo>;

    fn get_repo_name(&self) -> String;

    fn as_advisory_provider(&self) -> Option<&dyn AdvisoryProviderInterface> {
        None
    }

    fn as_any(&self) -> &dyn std::any::Any;
}
