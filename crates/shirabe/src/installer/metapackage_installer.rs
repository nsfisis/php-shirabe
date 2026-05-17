//! ref: composer/src/Composer/Installer/MetapackageInstaller.php

use crate::dependency_resolver::operation::install_operation::InstallOperation;
use crate::dependency_resolver::operation::uninstall_operation::UninstallOperation;
use crate::dependency_resolver::operation::update_operation::UpdateOperation;
use crate::installer::installer_interface::InstallerInterface;
use crate::io::io_interface;
use crate::io::io_interface::IOInterface;
use crate::package::package_interface::PackageInterface;
use crate::repository::installed_repository_interface::InstalledRepositoryInterface;
use anyhow::Result;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::InvalidArgumentException;

#[derive(Debug)]
pub struct MetapackageInstaller {
    io: Box<dyn IOInterface>,
}

impl MetapackageInstaller {
    pub fn new(io: Box<dyn IOInterface>) -> Self {
        Self { io }
    }
}

impl InstallerInterface for MetapackageInstaller {
    fn supports(&self, package_type: &str) -> bool {
        package_type == "metapackage"
    }

    fn is_installed(
        &self,
        repo: &dyn InstalledRepositoryInterface,
        package: &dyn PackageInterface,
    ) -> bool {
        repo.has_package(package)
    }

    fn download(
        &self,
        _package: &dyn PackageInterface,
        _prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<Box<dyn PromiseInterface>>> {
        Ok(Some(shirabe_external_packages::react::promise::resolve(
            None,
        )))
    }

    fn prepare(
        &self,
        _type: &str,
        _package: &dyn PackageInterface,
        _prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<Box<dyn PromiseInterface>>> {
        Ok(Some(shirabe_external_packages::react::promise::resolve(
            None,
        )))
    }

    fn cleanup(
        &self,
        _type: &str,
        _package: &dyn PackageInterface,
        _prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<Box<dyn PromiseInterface>>> {
        Ok(Some(shirabe_external_packages::react::promise::resolve(
            None,
        )))
    }

    fn install(
        &self,
        repo: &mut dyn InstalledRepositoryInterface,
        package: &dyn PackageInterface,
    ) -> Result<Option<Box<dyn PromiseInterface>>> {
        self.io.write_error(
            &format!("  - {}", InstallOperation::format(package, false)),
            true,
            io_interface::NORMAL,
        );

        repo.add_package(package.clone_box());

        Ok(Some(shirabe_external_packages::react::promise::resolve(
            None,
        )))
    }

    fn update(
        &self,
        repo: &mut dyn InstalledRepositoryInterface,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
    ) -> Result<Option<Box<dyn PromiseInterface>>> {
        if !repo.has_package(initial) {
            return Err(InvalidArgumentException {
                message: format!("Package is not installed: {}", initial),
                code: 0,
            }
            .into());
        }

        self.io.write_error(
            &format!("  - {}", UpdateOperation::format(initial, target, false)),
            true,
            io_interface::NORMAL,
        );

        repo.remove_package(initial);
        repo.add_package(target.clone_box());

        Ok(Some(shirabe_external_packages::react::promise::resolve(
            None,
        )))
    }

    fn uninstall(
        &self,
        repo: &mut dyn InstalledRepositoryInterface,
        package: &dyn PackageInterface,
    ) -> Result<Option<Box<dyn PromiseInterface>>> {
        if !repo.has_package(package) {
            return Err(InvalidArgumentException {
                message: format!("Package is not installed: {}", package),
                code: 0,
            }
            .into());
        }

        self.io.write_error(
            &format!("  - {}", UninstallOperation::format(package, false)),
            true,
            io_interface::NORMAL,
        );

        repo.remove_package(package);

        Ok(Some(shirabe_external_packages::react::promise::resolve(
            None,
        )))
    }

    fn get_install_path(&self, _package: &dyn PackageInterface) -> Option<String> {
        None
    }
}
