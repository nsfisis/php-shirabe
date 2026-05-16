//! ref: composer/src/Composer/Repository/WritableRepositoryInterface.php

use crate::installer::installation_manager::InstallationManager;
use crate::package::package_interface::PackageInterface;
use crate::repository::repository_interface::RepositoryInterface;
use anyhow::Result;

pub trait WritableRepositoryInterface: RepositoryInterface {
    fn write(&mut self, dev_mode: bool, installation_manager: &InstallationManager) -> Result<()>;

    fn add_package(&mut self, package: Box<dyn PackageInterface>) -> Result<()>;

    fn remove_package(&mut self, package: &dyn PackageInterface) -> Result<()>;

    fn get_canonical_packages(&self) -> Vec<Box<dyn PackageInterface>>;

    fn reload(&mut self);

    fn set_dev_package_names(&mut self, dev_package_names: Vec<String>);

    fn get_dev_package_names(&self) -> &Vec<String>;
}
