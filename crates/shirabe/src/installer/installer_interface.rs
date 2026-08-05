//! ref: composer/src/Composer/Installer/InstallerInterface.php

use crate::installer::BinaryPresenceInterface;
use crate::installer::PluginInstaller;
use crate::package::PackageInterfaceHandle;
use crate::repository::InstalledRepositoryInterfaceHandle;
use shirabe_php_shim::PhpMixed;

#[async_trait::async_trait(?Send)]
pub trait InstallerInterface: std::fmt::Debug {
    fn supports(&self, package_type: &str) -> anyhow::Result<bool>;

    fn is_installed(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<bool>;

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

    // install/update/uninstall take the repository as a shared handle: the concurrent operation
    // chains share it (and re-entrant flows like plugin registration reach the same repository
    // through RepositoryManager), so implementations must borrow it only in synchronous
    // sections (never across an await).
    async fn install(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn update(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn uninstall(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
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
