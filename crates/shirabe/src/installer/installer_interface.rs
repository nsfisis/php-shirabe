//! ref: composer/src/Composer/Installer/InstallerInterface.php

use crate::installer::BinaryPresenceInterface;
use crate::installer::PluginInstaller;
use crate::package::PackageInterfaceHandle;
use crate::repository::InstalledRepositoryInterface;
use shirabe_php_shim::PhpMixed;

#[async_trait::async_trait(?Send)]
pub trait InstallerInterface: std::fmt::Debug {
    fn supports(&self, package_type: &str) -> bool;

    fn is_installed(
        &mut self,
        repo: &dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> bool;

    async fn download(
        &mut self,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn prepare(
        &mut self,
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
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>>;

    fn get_install_path(&mut self, package: PackageInterfaceHandle) -> Option<String>;

    fn as_binary_presence_interface(&mut self) -> Option<&mut dyn BinaryPresenceInterface> {
        None
    }

    fn as_plugin_installer_mut(&mut self) -> Option<&mut PluginInstaller> {
        None
    }
}
