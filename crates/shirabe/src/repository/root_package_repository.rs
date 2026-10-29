//! ref: composer/src/Composer/Repository/RootPackageRepository.php

use crate::package::{BasePackageHandle, PackageInterfaceHandle, RootPackageInterfaceHandle};
use crate::repository::{
    ArrayRepository, ProviderInfo, RepositoryInterface, RepositoryInterfaceWeakHandle, SearchResult,
};
use bstr::{BStr, BString};
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

    pub fn get_repo_name(&self) -> BString {
        bstr::BString::from("root package repo".to_string())
    }
}

impl RepositoryInterface for RootPackageRepository {
    fn count(&mut self) -> anyhow::Result<usize> {
        self.inner.count()
    }

    fn has_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<bool> {
        self.inner.has_package(package)
    }

    fn find_package(
        &mut self,
        name: &BStr,
        constraint: crate::repository::FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>> {
        self.inner.find_package(name, constraint)
    }

    fn find_packages(
        &mut self,
        name: &BStr,
        constraint: Option<crate::repository::FindPackageConstraint>,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        self.inner.find_packages(name, constraint)
    }

    fn get_packages(&mut self) -> anyhow::Result<Vec<BasePackageHandle>> {
        self.inner.get_packages()
    }

    fn load_packages(
        &mut self,
        package_name_map: IndexMap<BString, Option<shirabe_semver::constraint::AnyConstraint>>,
        acceptable_stabilities: IndexMap<BString, i64>,
        stability_flags: IndexMap<BString, i64>,
        already_loaded: IndexMap<BString, IndexMap<BString, PackageInterfaceHandle>>,
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
        Ok(RootPackageRepository::get_repo_name(self))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_self_handle(&self, weak: RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }
}
