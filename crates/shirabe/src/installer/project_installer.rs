//! ref: composer/src/Composer/Installer/ProjectInstaller.php sha1:922f8e9e0da328fb8b95a8493cb2bb08579f62c0

use crate::downloader::DownloadManagerInterface;
use crate::installer::InstallerInterface;
use crate::package::PackageInterfaceHandle;
use crate::repository::InstalledRepositoryInterfaceHandle;
use crate::util::Filesystem;
use bstr::{BStr, BString};
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, bformat, path_from_bytes, path_to_bytes, rtrim_chars, strtr,
};

#[derive(Debug)]
pub struct ProjectInstaller {
    install_path: std::path::PathBuf,
    download_manager: std::rc::Rc<std::cell::RefCell<dyn DownloadManagerInterface>>,
    filesystem: std::rc::Rc<std::cell::RefCell<Filesystem>>,
}

impl ProjectInstaller {
    pub fn new(
        install_path: &std::path::Path,
        dm: std::rc::Rc<std::cell::RefCell<dyn DownloadManagerInterface>>,
        fs: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    ) -> Self {
        let install_path = std::path::PathBuf::from(path_from_bytes(&bformat!(
            "{}/",
            rtrim_chars(strtr(path_to_bytes(install_path), "\\", "/"), "/")
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
            return Err(InvalidArgumentException::new(bformat!(
                "Project directory {install_path} is not empty."
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
        r#type: &BStr,
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
        r#type: &BStr,
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
        Err(InvalidArgumentException::new(BString::from("not supported")).into())
    }

    async fn uninstall(
        &self,
        _repo: &InstalledRepositoryInterfaceHandle,
        _package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Err(InvalidArgumentException::new(BString::from("not supported")).into())
    }

    fn get_install_path(&self, _package: PackageInterfaceHandle) -> Option<std::path::PathBuf> {
        Some(self.install_path.clone())
    }
}
