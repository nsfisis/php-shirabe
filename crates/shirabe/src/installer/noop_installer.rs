//! ref: composer/src/Composer/Installer/NoopInstaller.php

use crate::installer::InstallerInterface;
use crate::package::PackageInterface;
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
        package: &dyn PackageInterface,
    ) -> bool {
        repo.has_package(package)
    }

    async fn download(
        &self,
        _package: &dyn PackageInterface,
        _prev_package: Option<&dyn PackageInterface>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn prepare(
        &self,
        _type: &str,
        _package: &dyn PackageInterface,
        _prev_package: Option<&dyn PackageInterface>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn cleanup(
        &self,
        _type: &str,
        _package: &dyn PackageInterface,
        _prev_package: Option<&dyn PackageInterface>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn install(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        package: &dyn PackageInterface,
    ) -> anyhow::Result<Option<PhpMixed>> {
        if !repo.has_package(package) {
            repo.add_package(package.clone_package_box());
        }

        Ok(None)
    }

    async fn update(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
    ) -> anyhow::Result<Option<PhpMixed>> {
        if !repo.has_package(initial) {
            return Err(InvalidArgumentException {
                message: format!("Package is not installed: {}", initial),
                code: 0,
            }
            .into());
        }

        repo.remove_package(initial);
        if !repo.has_package(target) {
            repo.add_package(target.clone_package_box());
        }

        Ok(None)
    }

    async fn uninstall(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        package: &dyn PackageInterface,
    ) -> anyhow::Result<Option<PhpMixed>> {
        if !repo.has_package(package) {
            return Err(InvalidArgumentException {
                message: format!("Package is not installed: {}", package),
                code: 0,
            }
            .into());
        }
        repo.remove_package(package);

        Ok(None)
    }

    fn get_install_path(&self, package: &dyn PackageInterface) -> Option<String> {
        let target_dir = package.get_target_dir();
        let pretty_name = package.get_pretty_name();

        Some(if let Some(dir) = target_dir {
            format!("{}/{}", pretty_name, dir)
        } else {
            pretty_name.to_string()
        })
    }
}
