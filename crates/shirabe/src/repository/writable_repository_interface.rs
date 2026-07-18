//! ref: composer/src/Composer/Repository/WritableRepositoryInterface.php

use crate::installer::InstallationManager;
use crate::package::PackageInterfaceHandle;
use crate::repository::RepositoryInterface;

pub trait WritableRepositoryInterface: RepositoryInterface {
    fn write(
        &mut self,
        dev_mode: bool,
        installation_manager: &InstallationManager,
    ) -> anyhow::Result<()>;

    fn add_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<()>;

    fn remove_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<()>;

    fn get_canonical_packages(&mut self) -> anyhow::Result<Vec<PackageInterfaceHandle>>;

    fn reload(&mut self) -> anyhow::Result<()>;

    fn set_dev_package_names(&mut self, dev_package_names: Vec<String>);

    fn get_dev_package_names(&self) -> &Vec<String>;
}
