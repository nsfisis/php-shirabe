//! ref: composer/src/Composer/Installer/LibraryInstaller.php

use std::any::Any;

use anyhow::Result;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{
    InvalidArgumentException, LogicException, PhpMixed, dirname, is_dir, is_link, preg_quote,
    realpath, rmdir, rtrim, strpos,
};

use crate::composer::PartialComposerWeakHandle;
use crate::downloader::DownloadManager;
use crate::installer::BinaryInstaller;
use crate::installer::BinaryPresenceInterface;
use crate::installer::InstallerInterface;
use crate::io::IOInterface;
use crate::package::PackageInterfaceHandle;
use crate::repository::InstalledRepositoryInterface;
use crate::util::Filesystem;
use crate::util::Platform;
use crate::util::Silencer;

/// Package installation manager.
#[derive(Debug)]
pub struct LibraryInstaller {
    pub(crate) composer: PartialComposerWeakHandle,
    pub(crate) vendor_dir: String,
    pub(crate) download_manager: Option<std::rc::Rc<std::cell::RefCell<DownloadManager>>>,
    pub(crate) io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    pub(crate) r#type: Option<String>,
    pub(crate) filesystem: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    pub(crate) binary_installer: BinaryInstaller,
}

impl LibraryInstaller {
    /// Initializes library installer.
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        composer: PartialComposerWeakHandle,
        r#type: Option<String>,
        filesystem: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
        binary_installer: Option<BinaryInstaller>,
    ) -> Self {
        let composer_rc = composer
            .upgrade()
            .expect("LibraryInstaller must lives longer than Composer");

        let download_manager = composer_rc
            .as_full()
            .map(|full| full.borrow().get_download_manager());

        let composer_ref = composer_rc.borrow_partial();

        let filesystem = filesystem
            .unwrap_or_else(|| std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(None))));
        let vendor_dir = rtrim(
            &composer_ref
                .get_config()
                .borrow_mut()
                .get_str("vendor-dir")
                .unwrap_or_default(),
            Some("/"),
        );
        let binary_installer = binary_installer.unwrap_or_else(|| {
            BinaryInstaller::new(
                io.clone(),
                rtrim(
                    &composer_ref
                        .get_config()
                        .borrow_mut()
                        .get_str("bin-dir")
                        .unwrap_or_default(),
                    Some("/"),
                ),
                composer_ref
                    .get_config()
                    .borrow_mut()
                    .get_str("bin-compat")
                    .unwrap_or_default(),
                Some(filesystem.clone()),
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
    pub fn ensure_binaries_presence(&mut self, package: PackageInterfaceHandle) {
        let install_path = self.get_install_path(package.clone()).unwrap();
        self.binary_installer
            .install_binaries(package, &install_path, false);
    }

    /// Returns the base path of the package without target-dir path
    ///
    /// It is used for BC as getInstallPath tends to be overridden by
    /// installer plugins but not getPackageBasePath
    pub(crate) fn get_package_base_path(&mut self, package: PackageInterfaceHandle) -> String {
        let install_path = self.get_install_path(package.clone()).unwrap();
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
    pub(crate) async fn install_code(
        &mut self,
        package: PackageInterfaceHandle,
    ) -> Result<Option<PhpMixed>> {
        let download_path = self.get_install_path(package.clone()).unwrap();

        self.get_download_manager()
            .borrow()
            .install(package, &download_path)
            .await
    }

    /// @return PromiseInterface|null
    /// @phpstan-return PromiseInterface<void|null>|null
    pub(crate) async fn update_code(
        &mut self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
    ) -> Result<Option<PhpMixed>> {
        let initial_download_path = self.get_install_path(initial.clone()).unwrap();
        let target_download_path = self.get_install_path(target.clone()).unwrap();
        if target_download_path != initial_download_path {
            // if the target and initial dirs intersect, we force a remove + install
            // to avoid the rename wiping the target dir as part of the initial dir cleanup
            if strpos(&initial_download_path, &target_download_path) == Some(0)
                || strpos(&target_download_path, &initial_download_path) == Some(0)
            {
                // PHP: return $this->removeCode($initial)->then(fn () => $this->installCode($target));
                let _ = self.remove_code(initial).await?;
                return self.install_code(target).await;
            }

            self.filesystem
                .borrow_mut()
                .rename(&initial_download_path, &target_download_path);
        }

        self.get_download_manager()
            .borrow()
            .update(initial, target, &target_download_path)
            .await
    }

    /// @return PromiseInterface|null
    /// @phpstan-return PromiseInterface<void|null>|null
    pub(crate) async fn remove_code(
        &mut self,
        package: PackageInterfaceHandle,
    ) -> Result<Option<PhpMixed>> {
        let download_path = self.get_package_base_path(package.clone());

        self.get_download_manager()
            .borrow()
            .remove(package, &download_path)
            .await
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

#[async_trait::async_trait(?Send)]
impl InstallerInterface for LibraryInstaller {
    fn supports(&self, package_type: &str) -> bool {
        match &self.r#type {
            Some(t) => package_type == t,
            None => true,
        }
    }

    fn is_installed(
        &mut self,
        repo: &dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> bool {
        if !repo.has_package(package.clone()) {
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

    async fn download(
        &mut self,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        self.initialize_vendor_dir();
        let download_path = self.get_install_path(package.clone()).unwrap();

        self.get_download_manager()
            .borrow()
            .download(package, &download_path, prev_package)
            .await
    }

    async fn prepare(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        self.initialize_vendor_dir();
        let download_path = self.get_install_path(package.clone()).unwrap();

        self.get_download_manager()
            .borrow()
            .prepare(r#type, package, &download_path, prev_package)
            .await
    }

    async fn cleanup(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        self.initialize_vendor_dir();
        let download_path = self.get_install_path(package.clone()).unwrap();

        self.get_download_manager()
            .borrow()
            .cleanup(r#type, package, &download_path, prev_package)
            .await
    }

    async fn install(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> Result<Option<PhpMixed>> {
        self.initialize_vendor_dir();
        let download_path = self.get_install_path(package.clone()).unwrap();

        // remove the binaries if it appears the package files are missing
        if !Filesystem::is_readable(&download_path) && repo.has_package(package.clone()) {
            self.binary_installer.remove_binaries(package.clone());
        }

        let _ = self.install_code(package.clone()).await?;

        let install_path = self.get_install_path(package.clone()).unwrap();
        self.binary_installer
            .install_binaries(package.clone(), &install_path, true);
        if !repo.has_package(package.clone()) {
            repo.add_package(PackageInterfaceHandle::dup(&package));
        }

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

        self.initialize_vendor_dir();

        self.binary_installer.remove_binaries(initial.clone());
        let _ = self.update_code(initial.clone(), target.clone()).await?;

        let install_path = self.get_install_path(target.clone()).unwrap();
        self.binary_installer
            .install_binaries(target.clone(), &install_path, true);
        repo.remove_package(initial.clone());
        if !repo.has_package(target.clone()) {
            repo.add_package(PackageInterfaceHandle::dup(&target));
        }

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

        let _ = self.remove_code(package.clone()).await?;

        let download_path = self.get_package_base_path(package.clone());
        self.binary_installer.remove_binaries(package.clone());
        repo.remove_package(package.clone());

        if strpos(&package.get_name(), "/").map_or(false, |pos| pos != 0) {
            let package_vendor_dir = dirname(&download_path);
            if is_dir(&package_vendor_dir)
                && self.filesystem.borrow().is_dir_empty(&package_vendor_dir)
            {
                let _ = Silencer::call(|| {
                    rmdir(&package_vendor_dir);
                    Ok(())
                });
            }
        }

        Ok(None)
    }

    fn get_install_path(&mut self, package: PackageInterfaceHandle) -> Option<String> {
        self.initialize_vendor_dir();

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

    fn as_binary_presence_interface(&mut self) -> Option<&mut dyn BinaryPresenceInterface> {
        Some(self)
    }
}

impl BinaryPresenceInterface for LibraryInstaller {
    fn ensure_binaries_presence(&mut self, package: PackageInterfaceHandle) {
        LibraryInstaller::ensure_binaries_presence(self, package);
    }
}
