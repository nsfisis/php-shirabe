//! ref: composer/src/Composer/Installer/ProjectInstaller.php

use crate::downloader::DownloadManager;
use crate::installer::InstallerInterface;
use crate::package::PackageInterface;
use crate::package::PackageInterfaceHandle;
use crate::repository::InstalledRepositoryInterface;
use crate::util::Filesystem;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed};

#[derive(Debug)]
pub struct ProjectInstaller {
    install_path: String,
    download_manager: std::rc::Rc<std::cell::RefCell<DownloadManager>>,
    filesystem: std::rc::Rc<std::cell::RefCell<Filesystem>>,
}

impl ProjectInstaller {
    pub fn new(
        install_path: &str,
        dm: std::rc::Rc<std::cell::RefCell<DownloadManager>>,
        fs: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    ) -> Self {
        let install_path = format!("{}/", install_path.replace('\\', "/").trim_end_matches('/'));
        Self {
            install_path,
            download_manager: dm,
            filesystem: fs,
        }
    }
}

#[async_trait::async_trait(?Send)]
impl InstallerInterface for ProjectInstaller {
    fn supports(&self, _package_type: &str) -> bool {
        true
    }

    fn is_installed(
        &self,
        _repo: &dyn InstalledRepositoryInterface,
        _package: &dyn PackageInterface,
    ) -> bool {
        false
    }

    async fn download(
        &self,
        package: &dyn PackageInterface,
        prev_package: Option<&dyn PackageInterface>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let install_path = &self.install_path;
        if std::path::Path::new(install_path).exists()
            && !self.filesystem.borrow().is_dir_empty(install_path)
        {
            return Err(InvalidArgumentException {
                message: format!("Project directory {} is not empty.", install_path),
                code: 0,
            }
            .into());
        }
        if !std::path::Path::new(install_path).is_dir() {
            std::fs::create_dir_all(install_path)?;
        }

        self.download_manager
            .borrow()
            .download(package, install_path, prev_package)
            .await
    }

    async fn prepare(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        prev_package: Option<&dyn PackageInterface>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.download_manager
            .borrow()
            .prepare(r#type, package, &self.install_path, prev_package)
            .await
    }

    async fn cleanup(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        prev_package: Option<&dyn PackageInterface>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.download_manager
            .borrow()
            .cleanup(r#type, package, &self.install_path, prev_package)
            .await
    }

    async fn install(
        &mut self,
        _repo: &mut dyn InstalledRepositoryInterface,
        package: &PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.download_manager
            .borrow()
            .install(
                package.as_rc().borrow().as_package_interface(),
                &self.install_path,
            )
            .await
    }

    async fn update(
        &mut self,
        _repo: &mut dyn InstalledRepositoryInterface,
        _initial: &PackageInterfaceHandle,
        _target: &PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Err(InvalidArgumentException {
            message: "not supported".to_string(),
            code: 0,
        }
        .into())
    }

    async fn uninstall(
        &mut self,
        _repo: &mut dyn InstalledRepositoryInterface,
        _package: &PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Err(InvalidArgumentException {
            message: "not supported".to_string(),
            code: 0,
        }
        .into())
    }

    fn get_install_path(&self, _package: &dyn PackageInterface) -> Option<String> {
        Some(self.install_path.clone())
    }
}
