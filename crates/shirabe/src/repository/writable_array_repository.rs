//! ref: composer/src/Composer/Repository/WritableArrayRepository.php

use crate::installer::InstallationManager;
use crate::repository::ArrayRepository;
use crate::repository::RepositoryInterface;
use anyhow::Result;
use shirabe_php_shim::Countable;

#[derive(Debug)]
pub struct WritableArrayRepository {
    inner: ArrayRepository,
    pub(crate) dev_package_names: Vec<String>,
    dev_mode: Option<bool>,
}

impl WritableArrayRepository {
    pub fn new(packages: Vec<crate::package::PackageInterfaceHandle>) -> Result<Self> {
        Ok(Self {
            inner: ArrayRepository::new(packages)?,
            dev_package_names: Vec::new(),
            dev_mode: None,
        })
    }

    /// Returns true if dev requirements were installed, false if --no-dev was used, None if yet unknown.
    pub fn get_dev_mode(&self) -> Option<bool> {
        self.dev_mode
    }

    pub fn set_dev_package_names(&mut self, dev_package_names: Vec<String>) {
        self.dev_package_names = dev_package_names;
    }

    pub fn get_dev_package_names(&self) -> &Vec<String> {
        &self.dev_package_names
    }

    pub fn write(
        &mut self,
        dev_mode: bool,
        _installation_manager: &InstallationManager,
    ) -> Result<()> {
        self.dev_mode = Some(dev_mode);
        Ok(())
    }

    pub fn reload(&mut self) {
        self.dev_mode = None;
    }

    pub fn add_package(&mut self, package: crate::package::PackageInterfaceHandle) -> Result<()> {
        self.inner.add_package(package)
    }

    pub fn remove_package(&mut self, package: &dyn crate::package::PackageInterface) -> Result<()> {
        let _ = package;
        // TODO(phase-b): delegate to ArrayRepository once it implements remove_package
        Ok(())
    }

    pub fn initialize(&mut self) -> Result<()> {
        // TODO(phase-b): inner ArrayRepository::initialize signature
        Ok(())
    }

    pub fn get_canonical_packages(&self) -> Vec<crate::package::PackageInterfaceHandle> {
        // TODO(phase-b): delegate to inner once it exposes get_canonical_packages
        Vec::new()
    }

    pub fn get_packages(&self) -> Vec<crate::package::BasePackageHandle> {
        crate::repository::RepositoryInterface::get_packages(&self.inner)
    }

    pub fn get_repo_name(&self) -> String {
        self.inner.get_repo_name()
    }

    pub fn count(&self) -> i64 {
        Countable::count(&self.inner)
    }
}
