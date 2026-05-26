//! ref: composer/src/Composer/Installer/InstallerInterface.php

use crate::package::PackageInterfaceHandle;
use crate::repository::InstalledRepositoryInterface;
use shirabe_php_shim::PhpMixed;

#[async_trait::async_trait(?Send)]
pub trait InstallerInterface: std::fmt::Debug {
    fn supports(&self, package_type: &str) -> bool;

    fn is_installed(
        &self,
        repo: &dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> bool;

    async fn download(
        &self,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn prepare(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn install(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn update(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn uninstall(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn cleanup(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>>;

    fn get_install_path(&self, package: PackageInterfaceHandle) -> Option<String>;

    fn clone_box(&self) -> Box<dyn InstallerInterface> {
        todo!()
    }
}
