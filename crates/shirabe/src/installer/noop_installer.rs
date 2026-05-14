//! ref: composer/src/Composer/Installer/NoopInstaller.php

use shirabe_php_shim::InvalidArgumentException;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use crate::installer::installer_interface::InstallerInterface;
use crate::package::package_interface::PackageInterface;
use crate::repository::installed_repository_interface::InstalledRepositoryInterface;

#[derive(Debug)]
pub struct NoopInstaller;

impl InstallerInterface for NoopInstaller {
    fn supports(&self, _package_type: &str) -> bool {
        true
    }

    fn is_installed(&self, repo: &dyn InstalledRepositoryInterface, package: &dyn PackageInterface) -> bool {
        repo.has_package(package)
    }

    fn download(&self, _package: &dyn PackageInterface, _prev_package: Option<&dyn PackageInterface>) -> anyhow::Result<Option<Box<dyn PromiseInterface>>> {
        Ok(Some(shirabe_external_packages::react::promise::resolve(None)))
    }

    fn prepare(&self, _type: &str, _package: &dyn PackageInterface, _prev_package: Option<&dyn PackageInterface>) -> anyhow::Result<Option<Box<dyn PromiseInterface>>> {
        Ok(Some(shirabe_external_packages::react::promise::resolve(None)))
    }

    fn cleanup(&self, _type: &str, _package: &dyn PackageInterface, _prev_package: Option<&dyn PackageInterface>) -> anyhow::Result<Option<Box<dyn PromiseInterface>>> {
        Ok(Some(shirabe_external_packages::react::promise::resolve(None)))
    }

    fn install(&self, repo: &mut dyn InstalledRepositoryInterface, package: &dyn PackageInterface) -> anyhow::Result<Option<Box<dyn PromiseInterface>>> {
        if !repo.has_package(package) {
            repo.add_package(package.clone_box());
        }

        Ok(Some(shirabe_external_packages::react::promise::resolve(None)))
    }

    fn update(&self, repo: &mut dyn InstalledRepositoryInterface, initial: &dyn PackageInterface, target: &dyn PackageInterface) -> anyhow::Result<Option<Box<dyn PromiseInterface>>> {
        if !repo.has_package(initial) {
            return Err(InvalidArgumentException {
                message: format!("Package is not installed: {}", initial),
                code: 0,
            }.into());
        }

        repo.remove_package(initial);
        if !repo.has_package(target) {
            repo.add_package(target.clone_box());
        }

        Ok(Some(shirabe_external_packages::react::promise::resolve(None)))
    }

    fn uninstall(&self, repo: &mut dyn InstalledRepositoryInterface, package: &dyn PackageInterface) -> anyhow::Result<Option<Box<dyn PromiseInterface>>> {
        if !repo.has_package(package) {
            return Err(InvalidArgumentException {
                message: format!("Package is not installed: {}", package),
                code: 0,
            }.into());
        }
        repo.remove_package(package);

        Ok(Some(shirabe_external_packages::react::promise::resolve(None)))
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
