//! ref: composer/src/Composer/Installer/ProjectInstaller.php

use crate::downloader::DownloadManagerInterface;
use crate::installer::InstallerInterface;
use crate::package::PackageInterfaceHandle;
use crate::repository::InstalledRepositoryInterfaceHandle;
use crate::util::Filesystem;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, bconcat, path_from_bytes, path_to_bytes, rtrim2, strtr,
};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ProjectInstaller {
    install_path: PathBuf,
    download_manager: std::rc::Rc<std::cell::RefCell<dyn DownloadManagerInterface>>,
    filesystem: std::rc::Rc<std::cell::RefCell<Filesystem>>,
}

impl ProjectInstaller {
    pub fn new(
        install_path: &Path,
        dm: std::rc::Rc<std::cell::RefCell<dyn DownloadManagerInterface>>,
        fs: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    ) -> Self {
        let install_path = PathBuf::from(path_from_bytes(&bconcat!(
            rtrim2(strtr(path_to_bytes(install_path), "\\", "/"), "/"),
            "/"
        )));
        Self {
            install_path,
            download_manager: dm,
            filesystem: fs,
        }
    }
}

#[async_trait::async_trait(?Send)]
impl InstallerInterface for ProjectInstaller {
    fn supports(&self, _package_type: &bstr::BStr) -> anyhow::Result<bool> {
        Ok(true)
    }

    fn is_installed(
        &self,
        _repo: &InstalledRepositoryInterfaceHandle,
        _package: PackageInterfaceHandle,
    ) -> anyhow::Result<bool> {
        Ok(false)
    }

    async fn download(
        &self,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let install_path = &self.install_path;
        if install_path.exists() && !self.filesystem.borrow().is_dir_empty(install_path) {
            return Err(InvalidArgumentException::new(format!(
                "Project directory {} is not empty.",
                install_path.display()
            ))
            .into());
        }
        if !install_path.is_dir() {
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
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.download_manager
            .borrow()
            .prepare(r#type, package, &self.install_path, prev_package)
            .await
    }

    async fn cleanup(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.download_manager
            .borrow()
            .cleanup(r#type, package, &self.install_path, prev_package)
            .await
    }

    async fn install(
        &self,
        _repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.download_manager
            .borrow()
            .install(package, &self.install_path)
            .await
    }

    async fn update(
        &self,
        _repo: &InstalledRepositoryInterfaceHandle,
        _initial: PackageInterfaceHandle,
        _target: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Err(InvalidArgumentException::new("not supported".to_string()).into())
    }

    async fn uninstall(
        &self,
        _repo: &InstalledRepositoryInterfaceHandle,
        _package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Err(InvalidArgumentException::new("not supported".to_string()).into())
    }

    fn get_install_path(&self, _package: PackageInterfaceHandle) -> Option<PathBuf> {
        Some(self.install_path.clone())
    }
}
