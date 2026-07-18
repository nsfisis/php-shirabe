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

    // install/update/uninstall take the repository behind a RefCell: the concurrent operation
    // chains share it, and implementations must borrow it only in synchronous sections (never
    // across an await).
    async fn install(
        &self,
        repo: &std::cell::RefCell<&mut dyn InstalledRepositoryInterface>,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn update(
        &self,
        repo: &std::cell::RefCell<&mut dyn InstalledRepositoryInterface>,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn uninstall(
        &self,
        repo: &std::cell::RefCell<&mut dyn InstalledRepositoryInterface>,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn cleanup(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>>;

    fn get_install_path(&self, package: PackageInterfaceHandle) -> Option<String>;

    fn as_binary_presence_interface(&self) -> Option<&dyn BinaryPresenceInterface> {
        None
    }

    fn as_plugin_installer(&self) -> Option<&PluginInstaller> {
        None
    }
}
