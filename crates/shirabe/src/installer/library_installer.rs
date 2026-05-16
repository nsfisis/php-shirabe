//! ref: composer/src/Composer/Installer/LibraryInstaller.php

use std::any::Any;

use anyhow::Result;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::{
    is_link, preg_quote, realpath, rmdir, rtrim, strpos, InvalidArgumentException, LogicException,
};

use crate::composer::Composer;
use crate::downloader::download_manager::DownloadManager;
use crate::installer::binary_installer::BinaryInstaller;
use crate::installer::binary_presence_interface::BinaryPresenceInterface;
use crate::installer::installer_interface::InstallerInterface;
use crate::io::io_interface::IOInterface;
use crate::package::package_interface::PackageInterface;
use crate::partial_composer::PartialComposer;
use crate::repository::installed_repository_interface::InstalledRepositoryInterface;
use crate::util::filesystem::Filesystem;
use crate::util::platform::Platform;
use crate::util::silencer::Silencer;

/// Package installation manager.
#[derive(Debug)]
pub struct LibraryInstaller {
    pub(crate) composer: PartialComposer,
    pub(crate) vendor_dir: String,
    pub(crate) download_manager: Option<DownloadManager>,
    pub(crate) io: Box<dyn IOInterface>,
    pub(crate) r#type: Option<String>,
    pub(crate) filesystem: Filesystem,
    pub(crate) binary_installer: BinaryInstaller,
}

impl LibraryInstaller {
    /// Initializes library installer.
    pub fn new(
        io: Box<dyn IOInterface>,
        composer: PartialComposer,
        r#type: Option<String>,
        filesystem: Option<Filesystem>,
        binary_installer: Option<BinaryInstaller>,
    ) -> Self {
        // PHP: $this->downloadManager = $composer instanceof Composer ? $composer->getDownloadManager() : null;
        let download_manager = if let Some(full_composer) =
            (composer.as_any() as &dyn Any).downcast_ref::<Composer>()
        {
            // TODO(phase-b): clone or borrow the DownloadManager from the full Composer
            Some(todo!("composer.get_download_manager() as DownloadManager"))
        } else {
            None
        };

        let filesystem = filesystem.unwrap_or_else(Filesystem::new);
        let vendor_dir = rtrim(
            // TODO(phase-b): composer.get_config().get("vendor-dir") returns a PhpMixed/String
            &composer.get_config().get("vendor-dir"),
            Some("/"),
        );
        let binary_installer = binary_installer.unwrap_or_else(|| {
            BinaryInstaller::new(
                // TODO(phase-b): pass io by reference/clone
                todo!("io reference"),
                rtrim(&composer.get_config().get("bin-dir"), Some("/")),
                composer.get_config().get("bin-compat"),
                // TODO(phase-b): pass filesystem reference
                todo!("filesystem reference"),
                vendor_dir.clone(),
            )
        });

        Self {
            composer,
            download_manager,
            io,
            r#type,
            filesystem,
            vendor_dir,
            binary_installer,
        }
    }

    /// Make sure binaries are installed for a given package.
    pub fn ensure_binaries_presence(&self, package: &dyn PackageInterface) {
        self.binary_installer
            .install_binaries(package, &self.get_install_path(package).unwrap(), false);
    }

    /// Returns the base path of the package without target-dir path
    ///
    /// It is used for BC as getInstallPath tends to be overridden by
    /// installer plugins but not getPackageBasePath
    pub(crate) fn get_package_base_path(&self, package: &dyn PackageInterface) -> String {
        let install_path = self.get_install_path(package).unwrap();
        let target_dir = package.get_target_dir();

        if let Some(target_dir) = target_dir {
            if !target_dir.is_empty() {
                return Preg::replace(
                    &format!(
                        "{{/*{}/?$}}",
                        preg_quote(&target_dir, None).replace('/', "/+")
                    ),
                    "",
                    &install_path,
                );
            }
        }

        install_path
    }

    /// @return PromiseInterface|null
    /// @phpstan-return PromiseInterface<void|null>|null
    pub(crate) fn install_code(
        &self,
        package: &dyn PackageInterface,
    ) -> Result<Option<Box<dyn PromiseInterface>>> {
        let download_path = self.get_install_path(package).unwrap();

        self.get_download_manager().install(package, &download_path)
    }

    /// @return PromiseInterface|null
    /// @phpstan-return PromiseInterface<void|null>|null
    pub(crate) fn update_code(
        &self,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
    ) -> Result<Option<Box<dyn PromiseInterface>>> {
        let initial_download_path = self.get_install_path(initial).unwrap();
        let target_download_path = self.get_install_path(target).unwrap();
        if target_download_path != initial_download_path {
            // if the target and initial dirs intersect, we force a remove + install
            // to avoid the rename wiping the target dir as part of the initial dir cleanup
            if strpos(&initial_download_path, &target_download_path) == Some(0)
                || strpos(&target_download_path, &initial_download_path) == Some(0)
            {
                let promise = self.remove_code(initial)?;
                let promise = match promise {
                    Some(p) => p,
                    None => shirabe_external_packages::react::promise::resolve(None),
                };

                return Ok(Some(promise.then(Box::new(
                    move || -> Result<Box<dyn PromiseInterface>> {
                        // TODO(phase-b): capture target/self into the closure
                        let promise = self.install_code(target)?;
                        if let Some(promise) = promise {
                            return Ok(promise);
                        }

                        Ok(shirabe_external_packages::react::promise::resolve(None))
                    },
                ))));
            }

            self.filesystem
                .rename(&initial_download_path, &target_download_path);
        }

        self.get_download_manager()
            .update(initial, target, &target_download_path)
    }

    /// @return PromiseInterface|null
    /// @phpstan-return PromiseInterface<void|null>|null
    pub(crate) fn remove_code(
        &self,
        package: &dyn PackageInterface,
    ) -> Result<Option<Box<dyn PromiseInterface>>> {
        let download_path = self.get_package_base_path(package);

        self.get_download_manager().remove(package, &download_path)
    }

    pub(crate) fn initialize_vendor_dir(&mut self) {
        self.filesystem.ensure_directory_exists(&self.vendor_dir);
        // TODO(phase-b): realpath returns Option<String>; PHP assigns to vendorDir even when false
        self.vendor_dir = realpath(&self.vendor_dir).unwrap();
    }

    pub(crate) fn get_download_manager(&self) -> &DownloadManager {
        // PHP: assert($this->downloadManager instanceof DownloadManager, new \LogicException(...))
        assert!(
            self.download_manager.is_some(),
            "{}",
            LogicException {
                message: format!(
                    "{} should be initialized with a fully loaded Composer instance to be able to install/... packages",
                    "LibraryInstaller",
                ),
                code: 0,
            }
            .message
        );

        self.download_manager.as_ref().unwrap()
    }
}

impl InstallerInterface for LibraryInstaller {
    fn supports(&self, package_type: &str) -> bool {
        match &self.r#type {
            Some(t) => package_type == t,
            None => true,
        }
    }

    fn is_installed(
        &self,
        repo: &dyn InstalledRepositoryInterface,
        package: &dyn PackageInterface,
    ) -> bool {
        if !repo.has_package(package) {
            return false;
        }

        let install_path = self.get_install_path(package).unwrap();

        if Filesystem::is_readable(&install_path) {
            return true;
        }

        if Platform::is_windows() && self.filesystem.is_junction(&install_path) {
            return true;
        }

        if is_link(&install_path) {
            if realpath(&install_path).is_none() {
                return false;
            }

            return true;
        }

        false
    }

    fn download(
        &self,
        package: &dyn PackageInterface,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<Box<dyn PromiseInterface>>> {
        // TODO(phase-b): initialize_vendor_dir requires &mut self
        // self.initialize_vendor_dir();
        let download_path = self.get_install_path(package).unwrap();

        self.get_download_manager()
            .download(package, &download_path, prev_package)
    }

    fn prepare(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<Box<dyn PromiseInterface>>> {
        // TODO(phase-b): initialize_vendor_dir requires &mut self
        // self.initialize_vendor_dir();
        let download_path = self.get_install_path(package).unwrap();

        self.get_download_manager()
            .prepare(r#type, package, &download_path, prev_package)
    }

    fn cleanup(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<Box<dyn PromiseInterface>>> {
        // TODO(phase-b): initialize_vendor_dir requires &mut self
        // self.initialize_vendor_dir();
        let download_path = self.get_install_path(package).unwrap();

        self.get_download_manager()
            .cleanup(r#type, package, &download_path, prev_package)
    }

    fn install(
        &self,
        repo: &mut dyn InstalledRepositoryInterface,
        package: &dyn PackageInterface,
    ) -> Result<Option<Box<dyn PromiseInterface>>> {
        // TODO(phase-b): initialize_vendor_dir requires &mut self
        // self.initialize_vendor_dir();
        let download_path = self.get_install_path(package).unwrap();

        // remove the binaries if it appears the package files are missing
        if !Filesystem::is_readable(&download_path) && repo.has_package(package) {
            self.binary_installer.remove_binaries(package);
        }

        let promise = self.install_code(package)?;
        let promise = match promise {
            Some(p) => p,
            None => shirabe_external_packages::react::promise::resolve(None),
        };

        let binary_installer = &self.binary_installer;
        let install_path = self.get_install_path(package).unwrap();

        // TODO(phase-b): capture binary_installer/install_path/package/repo into the closure
        Ok(Some(promise.then(Box::new(move || -> Result<()> {
            binary_installer.install_binaries(package, &install_path, true);
            if !repo.has_package(package) {
                repo.add_package(package.clone_box())?;
            }
            Ok(())
        }))))
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

        // TODO(phase-b): initialize_vendor_dir requires &mut self
        // self.initialize_vendor_dir();

        self.binary_installer.remove_binaries(initial);
        let promise = self.update_code(initial, target)?;
        let promise = match promise {
            Some(p) => p,
            None => shirabe_external_packages::react::promise::resolve(None),
        };

        let binary_installer = &self.binary_installer;
        let install_path = self.get_install_path(target).unwrap();

        // TODO(phase-b): capture binary_installer/install_path/target/initial/repo into the closure
        Ok(Some(promise.then(Box::new(move || -> Result<()> {
            binary_installer.install_binaries(target, &install_path, true);
            repo.remove_package(initial)?;
            if !repo.has_package(target) {
                repo.add_package(target.clone_box())?;
            }
            Ok(())
        }))))
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

        let promise = self.remove_code(package)?;
        let promise = match promise {
            Some(p) => p,
            None => shirabe_external_packages::react::promise::resolve(None),
        };

        let binary_installer = &self.binary_installer;
        let download_path = self.get_package_base_path(package);
        let filesystem = &self.filesystem;

        // TODO(phase-b): capture binary_installer/filesystem/download_path/package/repo into the closure
        Ok(Some(promise.then(Box::new(move || -> Result<()> {
            binary_installer.remove_binaries(package);
            repo.remove_package(package)?;

            if strpos(package.get_name(), "/").is_some() {
                let package_vendor_dir = shirabe_php_shim::dirname(&download_path);
                if shirabe_php_shim::is_dir(&package_vendor_dir)
                    && filesystem.is_dir_empty(&package_vendor_dir)
                {
                    Silencer::call(|| {
                        rmdir(&package_vendor_dir);
                        Ok(())
                    })?;
                }
            }
            Ok(())
        }))))
    }

    fn get_install_path(&self, package: &dyn PackageInterface) -> Option<String> {
        // TODO(phase-b): initialize_vendor_dir requires &mut self
        // self.initialize_vendor_dir();

        let base_path = format!(
            "{}{}",
            if !self.vendor_dir.is_empty() {
                format!("{}/", self.vendor_dir)
            } else {
                String::new()
            },
            package.get_pretty_name(),
        );
        let target_dir = package.get_target_dir();

        Some(if let Some(target_dir) = target_dir {
            if !target_dir.is_empty() {
                format!("{}/{}", base_path, target_dir)
            } else {
                base_path
            }
        } else {
            base_path
        })
    }
}

impl BinaryPresenceInterface for LibraryInstaller {
    fn ensure_binaries_presence(&self, package: &dyn PackageInterface) {
        LibraryInstaller::ensure_binaries_presence(self, package)
    }
}
