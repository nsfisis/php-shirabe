//! ref: composer/src/Composer/Installer/NoopInstaller.php

use crate::installer::InstallerInterface;
use crate::package::PackageInterfaceHandle;
use crate::repository::InstalledRepositoryInterfaceHandle;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed, bconcat, path_from_bytes};

#[derive(Debug)]
pub struct NoopInstaller;

#[async_trait::async_trait(?Send)]
impl InstallerInterface for NoopInstaller {
    fn supports(&self, _package_type: &bstr::BStr) -> anyhow::Result<bool> {
        Ok(true)
    }

    fn is_installed(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<bool> {
        repo.borrow_mut().has_package(package)
    }

    async fn download(
        &self,
        _package: PackageInterfaceHandle,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn prepare(
        &self,
        _type: &str,
        _package: PackageInterfaceHandle,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn cleanup(
        &self,
        _type: &str,
        _package: PackageInterfaceHandle,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn install(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let mut repo = repo.borrow_mut();
        if !repo.has_package(package.clone())? {
            repo.add_package(PackageInterfaceHandle::dup(&package));
        }

        Ok(None)
    }

    async fn update(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let mut repo = repo.borrow_mut();
        if !repo.has_package(initial.clone())? {
            return Err(InvalidArgumentException::new(format!(
                "Package is not installed: {}",
                initial.to_php_string().display_lossy()
            ))
            .into());
        }

        repo.remove_package(initial);
        if !repo.has_package(target.clone())? {
            repo.add_package(PackageInterfaceHandle::dup(&target));
        }

        Ok(None)
    }

    async fn uninstall(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let mut repo = repo.borrow_mut();
        if !repo.has_package(package.clone())? {
            return Err(InvalidArgumentException::new(format!(
                "Package is not installed: {}",
                package.to_php_string().display_lossy()
            ))
            .into());
        }
        repo.remove_package(package);

        Ok(None)
    }

    fn get_install_path(&self, package: PackageInterfaceHandle) -> Option<std::path::PathBuf> {
        let target_dir = package.get_target_dir();

        let install_path = if let Some(dir) = target_dir {
            bconcat!(package.get_pretty_name(), "/", dir)
        } else {
            package.get_pretty_name()
        };
        Some(std::path::PathBuf::from(path_from_bytes(&install_path)))
    }
}
