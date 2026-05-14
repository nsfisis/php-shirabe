//! ref: composer/src/Composer/Installer/ProjectInstaller.php

use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::InvalidArgumentException;
use crate::downloader::download_manager::DownloadManager;
use crate::installer::installer_interface::InstallerInterface;
use crate::package::package_interface::PackageInterface;
use crate::repository::installed_repository_interface::InstalledRepositoryInterface;
use crate::util::filesystem::Filesystem;

#[derive(Debug)]
pub struct ProjectInstaller {
    install_path: String,
    download_manager: DownloadManager,
    filesystem: Filesystem,
}

impl ProjectInstaller {
    pub fn new(install_path: &str, dm: DownloadManager, fs: Filesystem) -> Self {
        let install_path = format!("{}/", install_path.replace('\\', '/').trim_end_matches('/'));
        Self {
            install_path,
            download_manager: dm,
            filesystem: fs,
        }
    }
}

impl InstallerInterface for ProjectInstaller {
    fn supports(&self, _package_type: &str) -> bool {
        true
    }

    fn is_installed(&self, _repo: &dyn InstalledRepositoryInterface, _package: &dyn PackageInterface) -> bool {
        false
    }

    fn download(&self, package: &dyn PackageInterface, prev_package: Option<&dyn PackageInterface>) -> anyhow::Result<Option<Box<dyn PromiseInterface>>> {
        let install_path = &self.install_path;
        if std::path::Path::new(install_path).exists() && !self.filesystem.is_dir_empty(install_path) {
            return Err(InvalidArgumentException {
                message: format!("Project directory {} is not empty.", install_path),
                code: 0,
            }.into());
        }
        if !std::path::Path::new(install_path).is_dir() {
            std::fs::create_dir_all(install_path)?;
        }

        self.download_manager.download(package, install_path, prev_package)
    }

    fn prepare(&self, r#type: &str, package: &dyn PackageInterface, prev_package: Option<&dyn PackageInterface>) -> anyhow::Result<Option<Box<dyn PromiseInterface>>> {
        self.download_manager.prepare(r#type, package, &self.install_path, prev_package)
    }

    fn cleanup(&self, r#type: &str, package: &dyn PackageInterface, prev_package: Option<&dyn PackageInterface>) -> anyhow::Result<Option<Box<dyn PromiseInterface>>> {
        self.download_manager.cleanup(r#type, package, &self.install_path, prev_package)
    }

    fn install(&self, _repo: &mut dyn InstalledRepositoryInterface, package: &dyn PackageInterface) -> anyhow::Result<Option<Box<dyn PromiseInterface>>> {
        self.download_manager.install(package, &self.install_path)
    }

    fn update(&self, _repo: &mut dyn InstalledRepositoryInterface, _initial: &dyn PackageInterface, _target: &dyn PackageInterface) -> anyhow::Result<Option<Box<dyn PromiseInterface>>> {
        Err(InvalidArgumentException {
            message: "not supported".to_string(),
            code: 0,
        }.into())
    }

    fn uninstall(&self, _repo: &mut dyn InstalledRepositoryInterface, _package: &dyn PackageInterface) -> anyhow::Result<Option<Box<dyn PromiseInterface>>> {
        Err(InvalidArgumentException {
            message: "not supported".to_string(),
            code: 0,
        }.into())
    }

    fn get_install_path(&self, _package: &dyn PackageInterface) -> Option<String> {
        Some(self.install_path.clone())
    }
}
