//! ref: composer/src/Composer/Util/SyncHelper.php

use anyhow::Result;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use crate::downloader::download_manager::DownloadManager;
use crate::downloader::downloader_interface::DownloaderInterface;
use crate::package::package_interface::PackageInterface;
use crate::util::r#loop::Loop;

pub enum DownloaderOrManager<'a> {
    Interface(&'a dyn DownloaderInterface),
    Manager(&'a DownloadManager),
}

impl<'a> DownloaderOrManager<'a> {
    fn download(&self, package: &dyn PackageInterface, path: &str, prev_package: Option<&dyn PackageInterface>) -> Box<dyn PromiseInterface> {
        match self {
            Self::Interface(d) => d.download(package, path, prev_package),
            Self::Manager(d) => d.download(package, path, prev_package),
        }
    }

    fn prepare(&self, r#type: &str, package: &dyn PackageInterface, path: &str, prev_package: Option<&dyn PackageInterface>) -> Box<dyn PromiseInterface> {
        match self {
            Self::Interface(d) => d.prepare(r#type, package, path, prev_package),
            Self::Manager(d) => d.prepare(r#type, package, path, prev_package),
        }
    }

    fn install(&self, package: &dyn PackageInterface, path: &str) -> Box<dyn PromiseInterface> {
        match self {
            Self::Interface(d) => d.install(package, path),
            Self::Manager(d) => d.install(package, path),
        }
    }

    fn update(&self, package: &dyn PackageInterface, prev_package: &dyn PackageInterface, path: &str) -> Box<dyn PromiseInterface> {
        match self {
            Self::Interface(d) => d.update(package, prev_package, path),
            Self::Manager(d) => d.update(package, prev_package, path),
        }
    }

    fn cleanup(&self, r#type: &str, package: &dyn PackageInterface, path: &str, prev_package: Option<&dyn PackageInterface>) -> Box<dyn PromiseInterface> {
        match self {
            Self::Interface(d) => d.cleanup(r#type, package, path, prev_package),
            Self::Manager(d) => d.cleanup(r#type, package, path, prev_package),
        }
    }
}

pub struct SyncHelper;

impl SyncHelper {
    pub fn download_and_install_package_sync(
        r#loop: &Loop,
        downloader: DownloaderOrManager<'_>,
        path: String,
        package: &dyn PackageInterface,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<()> {
        let r#type = if prev_package.is_some() { "update" } else { "install" };

        let result: Result<()> = (|| {
            Self::r#await(r#loop, Some(downloader.download(package, &path, prev_package)))?;
            Self::r#await(r#loop, Some(downloader.prepare(r#type, package, &path, prev_package)))?;
            if r#type == "update" {
                if let Some(prev) = prev_package {
                    Self::r#await(r#loop, Some(downloader.update(package, prev, &path)))?;
                }
            } else {
                Self::r#await(r#loop, Some(downloader.install(package, &path)))?;
            }
            Ok(())
        })();

        if result.is_err() {
            Self::r#await(r#loop, Some(downloader.cleanup(r#type, package, &path, prev_package)))?;
            return result;
        }

        Self::r#await(r#loop, Some(downloader.cleanup(r#type, package, &path, prev_package)))?;
        Ok(())
    }

    pub fn r#await(r#loop: &Loop, promise: Option<Box<dyn PromiseInterface>>) -> Result<()> {
        if let Some(promise) = promise {
            r#loop.wait(vec![promise]);
        }
        Ok(())
    }
}
