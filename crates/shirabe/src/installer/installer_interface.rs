//! ref: composer/src/Composer/Installer/InstallerInterface.php

use crate::package::PackageInterface;
use crate::repository::InstalledRepositoryInterface;
use shirabe_external_packages::react::promise::PromiseInterface;

pub trait InstallerInterface: std::fmt::Debug {
    fn supports(&self, package_type: &str) -> bool;

    fn is_installed(
        &self,
        repo: &dyn InstalledRepositoryInterface,
        package: &dyn PackageInterface,
    ) -> bool;

    fn download(
        &self,
        package: &dyn PackageInterface,
        prev_package: Option<&dyn PackageInterface>,
    ) -> anyhow::Result<Option<Box<dyn PromiseInterface>>>;

    fn prepare(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        prev_package: Option<&dyn PackageInterface>,
    ) -> anyhow::Result<Option<Box<dyn PromiseInterface>>>;

    fn install(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        package: &dyn PackageInterface,
    ) -> anyhow::Result<Option<Box<dyn PromiseInterface>>>;

    fn update(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
    ) -> anyhow::Result<Option<Box<dyn PromiseInterface>>>;

    fn uninstall(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        package: &dyn PackageInterface,
    ) -> anyhow::Result<Option<Box<dyn PromiseInterface>>>;

    fn cleanup(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        prev_package: Option<&dyn PackageInterface>,
    ) -> anyhow::Result<Option<Box<dyn PromiseInterface>>>;

    fn get_install_path(&self, package: &dyn PackageInterface) -> Option<String>;

    fn clone_box(&self) -> Box<dyn InstallerInterface> {
        todo!()
    }
}
