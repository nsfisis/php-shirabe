//! ref: composer/src/Composer/Repository/RepositoryInterface.php

use crate::package::BasePackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::repository::AdvisoryProviderInterface;
use indexmap::IndexMap;
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

pub trait RepositoryInterface: std::fmt::Debug {
    // count/has_package take &mut self (and has_package returns Result) because PHP's
    // ArrayRepository::count()/hasPackage() late-bind $this->initialize() to the concrete
    // repository class, which lazily loads packages and can throw; lazy repositories need the
    // same guard here (see FilesystemRepository/PlatformRepository/ComposerRepository).
    fn count(&mut self) -> anyhow::Result<usize>;

    fn has_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<bool>;

    fn find_package(
        &mut self,
        name: &str,
        constraint: FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>>;

    fn find_packages(
        &mut self,
        name: &str,
        constraint: Option<FindPackageConstraint>,
    ) -> anyhow::Result<Vec<BasePackageHandle>>;

    fn get_packages(&mut self) -> anyhow::Result<Vec<BasePackageHandle>>;

    fn load_packages(
        &mut self,
        package_name_map: IndexMap<String, Option<AnyConstraint>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
    ) -> anyhow::Result<LoadPackagesResult>;

    fn search(
        &mut self,
        query: String,
        mode: i64,
        r#type: Option<String>,
    ) -> anyhow::Result<Vec<SearchResult>>;

    fn get_providers(
        &mut self,
        package_name: String,
    ) -> anyhow::Result<IndexMap<String, ProviderInfo>>;

    // PHP's getRepoName() can throw: ArrayRepository::getRepoName() counts through the
    // late-bound $this->initialize(), which is fallible in subclasses that read files
    // (FilesystemRepository, PackageRepository).
    fn get_repo_name(&self) -> anyhow::Result<String>;

    fn as_advisory_provider(&self) -> Option<&dyn AdvisoryProviderInterface> {
        None
    }

    fn as_advisory_provider_mut(&mut self) -> Option<&mut dyn AdvisoryProviderInterface> {
        None
    }

    // The `+ 'static` object bound lets `InstalledRepositoryInterfaceHandle` project a
    // `Ref`/`RefMut` through this method (`Ref::map` needs a lifetime-independent target).
    fn as_installed_repository_interface(
        &self,
    ) -> Option<&(dyn crate::repository::InstalledRepositoryInterface + 'static)> {
        None
    }

    fn as_installed_repository_interface_mut(
        &mut self,
    ) -> Option<&mut (dyn crate::repository::InstalledRepositoryInterface + 'static)> {
        None
    }

    fn as_writable_repository_interface_mut(
        &mut self,
    ) -> Option<&mut dyn crate::repository::WritableRepositoryInterface> {
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
