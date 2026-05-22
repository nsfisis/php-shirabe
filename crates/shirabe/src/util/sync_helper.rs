//! ref: composer/src/Composer/Util/SyncHelper.php

use crate::downloader::DownloadManager;
use crate::downloader::DownloaderInterface;
use crate::package::PackageInterface;
use crate::util::r#loop::Loop;
use anyhow::Result;
use shirabe_php_shim::PhpMixed;

pub enum DownloaderOrManager<'a> {
    Interface(&'a dyn DownloaderInterface),
    Manager(&'a std::rc::Rc<std::cell::RefCell<DownloadManager>>),
}

impl<'a> DownloaderOrManager<'a> {
    async fn download(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<PhpMixed>> {
        match self {
            Self::Interface(d) => d.download3(package, path, prev_package).await,
            Self::Manager(d) => d.borrow().download(package, path, prev_package).await,
        }
    }

    async fn prepare(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<PhpMixed>> {
        match self {
            Self::Interface(d) => d.prepare(r#type, package, path, prev_package).await,
            Self::Manager(d) => {
                d.borrow()
                    .prepare(r#type, package, path, prev_package)
                    .await
            }
        }
    }

    async fn install(
        &self,
        package: &dyn PackageInterface,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        match self {
            Self::Interface(d) => d.install2(package, path).await,
            Self::Manager(d) => d.borrow().install(package, path).await,
        }
    }

    async fn update(
        &self,
        package: &dyn PackageInterface,
        prev_package: &dyn PackageInterface,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        match self {
            Self::Interface(d) => d.update(package, prev_package, path).await,
            Self::Manager(d) => d.borrow().update(package, prev_package, path).await,
        }
    }

    async fn cleanup(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<PhpMixed>> {
        match self {
            Self::Interface(d) => d.cleanup(r#type, package, path, prev_package).await,
            Self::Manager(d) => {
                d.borrow()
                    .cleanup(r#type, package, path, prev_package)
                    .await
            }
        }
    }
}

pub struct SyncHelper;

impl SyncHelper {
    // TODO(phase-c-promise): synchronous wrapper driving now-async downloader calls via Self::await (loop.wait); needs async/loop boundary design.
    pub fn download_and_install_package_sync(
        r#loop: &std::rc::Rc<std::cell::RefCell<Loop>>,
        downloader: DownloaderOrManager<'_>,
        path: String,
        package: &dyn PackageInterface,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<()> {
        let r#type = if prev_package.is_some() {
            "update"
        } else {
            "install"
        };

        let result: Result<()> = (|| {
            Self::r#await(
                r#loop,
                Some(downloader.download(package, &path, prev_package)?),
            )?;
            Self::r#await(
                r#loop,
                Some(downloader.prepare(r#type, package, &path, prev_package)?),
            )?;
            if r#type == "update" {
                if let Some(prev) = prev_package {
                    Self::r#await(r#loop, Some(downloader.update(package, prev, &path)?))?;
                }
            } else {
                Self::r#await(r#loop, Some(downloader.install(package, &path)?))?;
            }
            Ok(())
        })();

        if result.is_err() {
            Self::r#await(
                r#loop,
                Some(downloader.cleanup(r#type, package, &path, prev_package)?),
            )?;
            return result;
        }

        Self::r#await(
            r#loop,
            Some(downloader.cleanup(r#type, package, &path, prev_package)?),
        )?;
        Ok(())
    }

    // TODO(phase-c-promise): loop-pump synchronous wait over a promise; driving mechanism needs design.
    pub fn r#await(
        r#loop: &std::rc::Rc<std::cell::RefCell<Loop>>,
        promise: Option<Box<dyn PromiseInterface>>,
    ) -> Result<()> {
        if let Some(promise) = promise {
            r#loop.borrow_mut().wait(vec![promise], None)?;
        }
        Ok(())
    }
}
