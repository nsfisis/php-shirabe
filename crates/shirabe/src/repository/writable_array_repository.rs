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
}
