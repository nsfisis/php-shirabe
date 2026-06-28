//! ref: composer/src/Composer/Repository/InstalledFilesystemRepository.php

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
use indexmap::IndexMap;
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug)]
pub struct InstalledFilesystemRepository {
    inner: FilesystemRepository,
    /// For testing only: when true, `reload` and `write` are no-ops, mirroring
    /// `Composer\Test\Mock\InstalledFilesystemRepositoryMock`. `false` in production.
    mock: bool,
}

impl InstalledFilesystemRepository {
    pub fn new(
        repository_file: JsonFile,
        dump_versions: bool,
        root_package: Option<RootPackageInterfaceHandle>,
        filesystem: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            inner: FilesystemRepository::new(
                repository_file,
                dump_versions,
                root_package,
                filesystem,
            )?,
            mock: false,
        })
    }

    /// For testing only: builds a repository whose `reload`/`write` are no-ops, mirroring
    /// `Composer\Test\Mock\InstalledFilesystemRepositoryMock`.
    pub fn __new_mock(
        repository_file: JsonFile,
        dump_versions: bool,
        root_package: Option<RootPackageInterfaceHandle>,
        filesystem: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            mock: true,
            ..Self::new(repository_file, dump_versions, root_package, filesystem)?
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
        installation_manager: &mut crate::installer::InstallationManager,
    ) -> anyhow::Result<()> {
        // For testing only (ref InstalledFilesystemRepositoryMock::write is a noop).
        if self.mock {
            return Ok(());
        }
        self.inner.write(dev_mode, installation_manager)
    }

    fn add_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<()> {
        self.inner.add_package(package)
    }

    fn remove_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<()> {
        self.inner.remove_package(package)
    }

    fn get_canonical_packages(&mut self) -> anyhow::Result<Vec<PackageInterfaceHandle>> {
        self.inner.get_canonical_packages()
    }

    fn reload(&mut self) -> anyhow::Result<()> {
        // For testing only (ref InstalledFilesystemRepositoryMock::reload is a noop).
        if self.mock {
            return Ok(());
        }
        self.inner.reload()
    }

    fn set_dev_package_names(&mut self, dev_package_names: Vec<String>) {
        self.inner.set_dev_package_names(dev_package_names);
    }

    fn get_dev_package_names(&self) -> &Vec<String> {
        self.inner.get_dev_package_names()
    }
}

impl RepositoryInterface for InstalledFilesystemRepository {
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
        self.inner.get_packages()
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
        format!("installed {}", self.inner.get_repo_name())
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
    fn as_writable_repository_interface_mut(
        &mut self,
    ) -> Option<&mut dyn crate::repository::WritableRepositoryInterface> {
        Some(self)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn set_self_handle(&self, weak: crate::repository::RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }
}
