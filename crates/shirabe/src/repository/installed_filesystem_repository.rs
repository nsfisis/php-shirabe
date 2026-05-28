//! ref: composer/src/Composer/Repository/InstalledFilesystemRepository.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::Countable;
use shirabe_semver::constraint::AnyConstraint;

use crate::json::JsonFile;
use crate::package::BasePackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::package::RootPackageInterfaceHandle;
use crate::repository::AdvisoryProviderInterface;
use crate::repository::FilesystemRepository;
use crate::repository::InstalledRepositoryInterface;
use crate::repository::WritableRepositoryInterface;
use crate::repository::{
    FindPackageConstraint, LoadPackagesResult, ProviderInfo, RepositoryInterface, SearchResult,
};
use crate::util::Filesystem;

#[derive(Debug)]
pub struct InstalledFilesystemRepository {
    inner: FilesystemRepository,
}

impl InstalledFilesystemRepository {
    pub fn new(
        repository_file: JsonFile,
        dump_versions: bool,
        root_package: Option<RootPackageInterfaceHandle>,
        filesystem: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
    ) -> Result<Self> {
        Ok(Self {
            inner: FilesystemRepository::new(
                repository_file,
                dump_versions,
                root_package,
                filesystem,
            )?,
        })
    }

    pub fn get_repo_name(&self) -> String {
        format!("installed {}", self.inner.get_repo_name())
    }
}

impl InstalledRepositoryInterface for InstalledFilesystemRepository {
    fn get_dev_mode(&self) -> Option<bool> {
        self.inner.get_dev_mode()
    }

    fn is_fresh(&self) -> bool {
        !self.inner.file.exists()
    }
}

impl WritableRepositoryInterface for InstalledFilesystemRepository {
    fn write(
        &mut self,
        dev_mode: bool,
        installation_manager: &crate::installer::InstallationManager,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn add_package(
        &mut self,
        package: crate::package::PackageInterfaceHandle,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn remove_package(
        &mut self,
        package: crate::package::PackageInterfaceHandle,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn get_canonical_packages(&self) -> Vec<crate::package::PackageInterfaceHandle> {
        todo!()
    }

    fn reload(&mut self) {
        todo!()
    }

    fn set_dev_package_names(&mut self, dev_package_names: Vec<String>) {
        todo!()
    }

    fn get_dev_package_names(&self) -> &Vec<String> {
        todo!()
    }
}

impl Countable for InstalledFilesystemRepository {
    fn count(&self) -> i64 {
        todo!()
    }
}

impl RepositoryInterface for InstalledFilesystemRepository {
    fn has_package(&self, _package: PackageInterfaceHandle) -> bool {
        todo!()
    }
    fn find_package(
        &self,
        _name: &str,
        _constraint: FindPackageConstraint,
    ) -> Option<BasePackageHandle> {
        todo!()
    }
    fn find_packages(
        &self,
        _name: &str,
        _constraint: Option<FindPackageConstraint>,
    ) -> Vec<BasePackageHandle> {
        todo!()
    }
    fn get_packages(&self) -> Vec<BasePackageHandle> {
        todo!()
    }
    fn load_packages(
        &self,
        _package_name_map: IndexMap<String, Option<AnyConstraint>>,
        _acceptable_stabilities: IndexMap<String, i64>,
        _stability_flags: IndexMap<String, i64>,
        _already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
    ) -> LoadPackagesResult {
        todo!()
    }
    fn search(&self, _query: String, _mode: i64, _type: Option<String>) -> Vec<SearchResult> {
        todo!()
    }
    fn get_providers(&self, _package_name: String) -> IndexMap<String, ProviderInfo> {
        todo!()
    }
    fn get_repo_name(&self) -> String {
        todo!()
    }
    fn as_advisory_provider(&self) -> Option<&dyn AdvisoryProviderInterface> {
        None
    }
    fn as_installed_repository_interface(&self) -> Option<&dyn InstalledRepositoryInterface> {
        Some(self)
    }
    fn as_installed_repository_interface_mut(
        &mut self,
    ) -> Option<&mut dyn InstalledRepositoryInterface> {
        Some(self)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        todo!()
    }
    fn set_self_handle(&self, weak: crate::repository::RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }
}
