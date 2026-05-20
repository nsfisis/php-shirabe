//! ref: composer/src/Composer/Installer/LibraryInstaller.php

use std::any::Any;

use anyhow::Result;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::react::promise::PromiseInterface;
use shirabe_php_shim::{
    InvalidArgumentException, LogicException, is_link, preg_quote, realpath, rmdir, rtrim, strpos,
};

use crate::composer::Composer;
use crate::downloader::DownloadManager;
use crate::installer::BinaryInstaller;
use crate::installer::BinaryPresenceInterface;
use crate::installer::InstallerInterface;
use crate::io::IOInterface;
use crate::package::PackageInterface;
use crate::partial_composer::PartialComposer;
use crate::repository::InstalledRepositoryInterface;
use crate::util::Filesystem;
use crate::util::Platform;
use crate::util::Silencer;

/// Package installation manager.
#[derive(Debug)]
pub struct LibraryInstaller {
    pub(crate) composer: PartialComposer,
    pub(crate) vendor_dir: String,
    pub(crate) download_manager: Option<std::rc::Rc<std::cell::RefCell<DownloadManager>>>,
    pub(crate) io: Box<dyn IOInterface>,
    pub(crate) r#type: Option<String>,
    pub(crate) filesystem: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    pub(crate) binary_installer: BinaryInstaller,
}

impl LibraryInstaller {
    /// Initializes library installer.
    pub fn new(
        io: Box<dyn IOInterface>,
        composer: PartialComposer,
        r#type: Option<String>,
        filesystem: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
        binary_installer: Option<BinaryInstaller>,
    ) -> Self {
        // PHP: $this->downloadManager = $composer instanceof Composer ? $composer->getDownloadManager() : null;
        // TODO(phase-b): PartialComposer cannot downcast to Composer in this Rust port.
        let download_manager: Option<std::rc::Rc<std::cell::RefCell<DownloadManager>>> =
            if let Some(_full_composer) = composer.as_any().downcast_ref::<Composer>() {
                // TODO(phase-b): clone or borrow the DownloadManager from the full Composer
                Some(todo!("composer.get_download_manager() as DownloadManager"))
            } else {
                None
            };

        let filesystem = filesystem
            .unwrap_or_else(|| std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(None))));
        let vendor_dir = rtrim(
            // TODO(phase-b): Config::get returns PhpMixed; coerce to String via get_str.
            &composer
                .get_config()
                .borrow_mut()
                .get_str("vendor-dir")
                .unwrap_or_default(),
            Some("/"),
        );
        let binary_installer = binary_installer.unwrap_or_else(|| {
            BinaryInstaller::new(
                // TODO(phase-b): pass io by reference/clone
                todo!("io reference"),
                rtrim(
                    &composer
                        .get_config()
                        .borrow_mut()
                        .get_str("bin-dir")
                        .unwrap_or_default(),
                    Some("/"),
                ),
                // TODO(phase-b): Config::get returns PhpMixed; coerce to String via get_str.
                composer
                    .get_config()
                    .borrow_mut()
                    .get_str("bin-compat")
                    .unwrap_or_default(),
                // TODO(phase-b): pass filesystem reference
                todo!("filesystem reference"),
                Some(vendor_dir.clone()),
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
    pub fn ensure_binaries_presence(&mut self, package: &dyn PackageInterface) {
        let install_path = self.get_install_path(package).unwrap();
        self.binary_installer
            .install_binaries(package, &install_path, false);
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
                let replaced = Preg::replace(
                    &format!(
                        "{{/*{}/?$}}",
                        preg_quote(&target_dir, None).replace('/', "/+")
                    ),
                    "",
                    &install_path,
                );
                return replaced.unwrap_or(install_path);
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

        Ok(Some(
            self.get_download_manager()
                .borrow()
                .install(package, &download_path)?,
        ))
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

                // TODO(phase-b): promise.then expects Option<Box<dyn FnOnce(Option<PhpMixed>) -> Option<PhpMixed>>>
                // arguments. Translating the original PHP closure (which captures &self and target)
                // requires restructuring; tracked separately.
                let _ = promise;
                return Ok(Some(todo!(
                    "promise.then(...) chain to install_code(target)"
                )));
            }

            self.filesystem
                .borrow_mut()
                .rename(&initial_download_path, &target_download_path);
        }

        Ok(Some(self.get_download_manager().borrow().update(
            initial,
            target,
            &target_download_path,
        )?))
    }

    /// @return PromiseInterface|null
    /// @phpstan-return PromiseInterface<void|null>|null
    pub(crate) fn remove_code(
        &self,
        package: &dyn PackageInterface,
    ) -> Result<Option<Box<dyn PromiseInterface>>> {
        let download_path = self.get_package_base_path(package);

        Ok(Some(
            self.get_download_manager()
                .borrow()
                .remove(package, &download_path)?,
        ))
    }

    pub(crate) fn initialize_vendor_dir(&mut self) {
        self.filesystem
            .borrow_mut()
            .ensure_directory_exists(&self.vendor_dir);
        // TODO(phase-b): realpath returns Option<String>; PHP assigns to vendorDir even when false
        self.vendor_dir = realpath(&self.vendor_dir).unwrap();
    }

    pub(crate) fn get_download_manager(&self) -> &std::rc::Rc<std::cell::RefCell<DownloadManager>> {
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

        if Platform::is_windows() && self.filesystem.borrow_mut().is_junction(&install_path) {
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

        Ok(Some(self.get_download_manager().borrow().download(
            package,
            &download_path,
            prev_package,
        )?))
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

        Ok(Some(self.get_download_manager().borrow().prepare(
            r#type,
            package,
            &download_path,
            prev_package,
        )?))
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

        Ok(Some(self.get_download_manager().borrow().cleanup(
            r#type,
            package,
            &download_path,
            prev_package,
        )?))
    }

    fn install(
        &mut self,
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

        // TODO(phase-b): promise.then expects Option<Box<dyn FnOnce(Option<PhpMixed>) -> Option<PhpMixed>>>
        // arguments. The original PHP closure captures &mut self/binary_installer/repo/package;
        // restructuring required.
        let _ = promise;
        Ok(Some(todo!(
            "promise.then(...) chain to install_binaries + repo.add_package"
        )))
    }

    fn update(
        &mut self,
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

        // TODO(phase-b): promise.then expects Option<Box<dyn FnOnce(Option<PhpMixed>) -> Option<PhpMixed>>>
        // arguments. Closure captures &mut self/binary_installer/repo/initial/target;
        // restructuring required.
        let _ = promise;
        Ok(Some(todo!(
            "promise.then(...) chain to install_binaries + repo updates"
        )))
    }

    fn uninstall(
        &mut self,
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

        // TODO(phase-b): promise.then expects Option<Box<dyn FnOnce(Option<PhpMixed>) -> Option<PhpMixed>>>
        // arguments. Closure captures binary_installer/filesystem/download_path/package/repo;
        // restructuring required.
        let _ = promise;
        Ok(Some(todo!(
            "promise.then(...) chain to remove_binaries/remove_package/rmdir"
        )))
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
    fn ensure_binaries_presence(&self, _package: &dyn PackageInterface) {
        // TODO(phase-b): trait takes &self but LibraryInstaller::ensure_binaries_presence
        // requires &mut self due to BinaryInstaller::install_binaries(&mut self, ...).
        // Revisit the trait or use interior mutability.
        todo!()
    }
}
