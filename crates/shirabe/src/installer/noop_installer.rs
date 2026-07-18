//! ref: composer/src/Composer/Installer/NoopInstaller.php

use crate::installer::InstallerInterface;
use crate::package::PackageInterfaceHandle;
use crate::repository::InstalledRepositoryInterface;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed};

#[derive(Debug)]
pub struct NoopInstaller;

#[async_trait::async_trait(?Send)]
impl InstallerInterface for NoopInstaller {
    fn supports(&self, _package_type: &str) -> bool {
        true
    }

    fn is_installed(
        &self,
        repo: &dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> bool {
        repo.has_package(package.clone())
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
        repo: &mut dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        if !repo.has_package(package.clone()) {
            repo.add_package(PackageInterfaceHandle::dup(&package));
        }

        Ok(None)
    }

    async fn update(
        &self,
        repo: &mut dyn InstalledRepositoryInterface,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        if !repo.has_package(initial.clone()) {
            return Err(InvalidArgumentException {
                message: format!("Package is not installed: {}", initial),
                code: 0,
            }
            .into());
        }

        repo.remove_package(initial.clone());
        if !repo.has_package(target.clone()) {
            repo.add_package(PackageInterfaceHandle::dup(&target));
        }

        Ok(None)
    }

    async fn uninstall(
        &self,
        repo: &mut dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        if !repo.has_package(package.clone()) {
            return Err(InvalidArgumentException {
                message: format!("Package is not installed: {}", package),
                code: 0,
            }
            .into());
        }
        repo.remove_package(package.clone());

        Ok(None)
    }

    fn get_install_path(&self, package: PackageInterfaceHandle) -> Option<String> {
        let target_dir = package.get_target_dir();
        let pretty_name = package.get_pretty_name();

        Some(if let Some(dir) = target_dir {
            format!("{}/{}", pretty_name, dir)
        } else {
            pretty_name
        })
    }
}
