//! ref: composer/src/Composer/Installer/MetapackageInstaller.php

use crate::dependency_resolver::operation::InstallOperation;
use crate::dependency_resolver::operation::UninstallOperation;
use crate::dependency_resolver::operation::UpdateOperation;
use crate::installer::InstallerInterface;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::io::io_interface;
use crate::package::PackageInterfaceHandle;
use crate::repository::InstalledRepositoryInterface;
use anyhow::Result;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed};

#[derive(Debug)]
pub struct MetapackageInstaller {
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
}

impl MetapackageInstaller {
    pub fn new(io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>) -> Self {
        Self { io }
    }
}

#[async_trait::async_trait(?Send)]
impl InstallerInterface for MetapackageInstaller {
    fn supports(&self, package_type: &str) -> bool {
        package_type == "metapackage"
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
    ) -> Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn prepare(
        &self,
        _type: &str,
        _package: PackageInterfaceHandle,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn cleanup(
        &self,
        _type: &str,
        _package: PackageInterfaceHandle,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn install(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> Result<Option<PhpMixed>> {
        self.io.write_error3(
            &format!("  - {}", InstallOperation::format(package.clone(), false)),
            true,
            io_interface::NORMAL,
        );

        repo.add_package(PackageInterfaceHandle::dup(&package));

        Ok(None)
    }

    async fn update(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
    ) -> Result<Option<PhpMixed>> {
        if !repo.has_package(initial.clone()) {
            return Err(InvalidArgumentException {
                message: format!("Package is not installed: {}", initial),
                code: 0,
            }
            .into());
        }

        self.io.write_error3(
            &format!(
                "  - {}",
                UpdateOperation::format(initial.clone(), target.clone(), false)
            ),
            true,
            io_interface::NORMAL,
        );

        repo.remove_package(initial.clone());
        repo.add_package(PackageInterfaceHandle::dup(&target));

        Ok(None)
    }

    async fn uninstall(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> Result<Option<PhpMixed>> {
        if !repo.has_package(package.clone()) {
            return Err(InvalidArgumentException {
                message: format!("Package is not installed: {}", package),
                code: 0,
            }
            .into());
        }

        self.io.write_error3(
            &format!("  - {}", UninstallOperation::format(package.clone(), false)),
            true,
            io_interface::NORMAL,
        );

        repo.remove_package(package.clone());

        Ok(None)
    }

    fn get_install_path(&self, _package: PackageInterfaceHandle) -> Option<String> {
        None
    }
}
