//! ref: composer/src/Composer/Repository/RepositoryInterface.php

use crate::package::BasePackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::repository::AdvisoryProviderInterface;
use indexmap::IndexMap;
use shirabe_php_shim::Countable;
use shirabe_semver::constraint::AnyConstraint;

pub enum FindPackageConstraint {
    String(String),
    Constraint(AnyConstraint),
}

impl Clone for FindPackageConstraint {
    fn clone(&self) -> Self {
        match self {
            Self::String(s) => Self::String(s.clone()),
            Self::Constraint(c) => Self::Constraint(c.clone()),
        }
    }
}

#[derive(Debug)]
pub struct LoadPackagesResult {
    pub names_found: Vec<String>,
    pub packages: IndexMap<String, BasePackageHandle>,
}

#[derive(Debug, Clone)]
pub enum AbandonedInfo {
    Replacement(String),
    Abandoned,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub name: String,
    pub description: Option<String>,
    pub abandoned: Option<AbandonedInfo>,
    pub url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub description: Option<String>,
    pub r#type: String,
}

pub const SEARCH_FULLTEXT: i64 = 0;
pub const SEARCH_NAME: i64 = 1;
pub const SEARCH_VENDOR: i64 = 2;

pub trait RepositoryInterface: Countable + std::fmt::Debug {
    fn has_package(&self, package: PackageInterfaceHandle) -> bool;

    fn find_package(
        &self,
        name: &str,
        constraint: FindPackageConstraint,
    ) -> Option<BasePackageHandle>;

    fn find_packages(
        &self,
        name: &str,
        constraint: Option<FindPackageConstraint>,
    ) -> Vec<BasePackageHandle>;

    fn get_packages(&self) -> Vec<BasePackageHandle>;

    fn load_packages(
        &self,
        package_name_map: IndexMap<String, Option<AnyConstraint>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
    ) -> LoadPackagesResult;

    fn search(&self, query: String, mode: i64, r#type: Option<String>) -> Vec<SearchResult>;

    fn get_providers(&self, package_name: String) -> IndexMap<String, ProviderInfo>;

    fn get_repo_name(&self) -> String;

    fn as_advisory_provider(&self) -> Option<&dyn AdvisoryProviderInterface> {
        None
    }

    fn as_installed_repository_interface(
        &self,
    ) -> Option<&dyn crate::repository::InstalledRepositoryInterface> {
        None
    }

    fn as_installed_repository_interface_mut(
        &mut self,
    ) -> Option<&mut dyn crate::repository::InstalledRepositoryInterface> {
        None
    }

    fn as_any(&self) -> &dyn std::any::Any;

    /// Injects this repository's own weak handle so that `add_package` can wire package ->
    /// repository back-references (PHP `setRepository($this)`). Called once when the repository is
    /// wrapped in a [`RepositoryInterfaceHandle`](crate::repository::RepositoryInterfaceHandle).
    /// Wrapper repositories forward the same weak (the outermost handle) to their inner
    /// `ArrayRepository`.
    fn set_self_handle(&self, weak: crate::repository::RepositoryInterfaceWeakHandle) {
        let _ = weak;
    }
}
