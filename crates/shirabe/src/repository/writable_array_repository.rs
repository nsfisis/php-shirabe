//! ref: composer/src/Composer/Repository/WritableArrayRepository.php

use crate::installer::installation_manager::InstallationManager;
use crate::repository::array_repository::ArrayRepository;
use anyhow::Result;

#[derive(Debug)]
pub struct WritableArrayRepository {
    inner: ArrayRepository,
    pub(crate) dev_package_names: Vec<String>,
    dev_mode: Option<bool>,
}

impl WritableArrayRepository {
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

    pub fn add_package(
        &mut self,
        package: Box<dyn crate::package::package_interface::PackageInterface>,
    ) -> Result<()> {
        self.inner.add_package(package)
    }

    pub fn remove_package(
        &mut self,
        package: &dyn crate::package::package_interface::PackageInterface,
    ) -> Result<()> {
        let _ = package;
        // TODO(phase-b): delegate to ArrayRepository once it implements remove_package
        Ok(())
    }

    pub fn initialize(&mut self) -> Result<()> {
        // TODO(phase-b): inner ArrayRepository::initialize signature
        Ok(())
    }

    pub fn get_canonical_packages(
        &self,
    ) -> Vec<Box<dyn crate::package::package_interface::PackageInterface>> {
        // TODO(phase-b): delegate to inner once it exposes get_canonical_packages
        Vec::new()
    }

    pub fn get_packages(
        &self,
    ) -> Vec<Box<dyn crate::package::package_interface::PackageInterface>> {
        // TODO(phase-b): delegate to inner ArrayRepository::get_packages
        Vec::new()
    }
}
