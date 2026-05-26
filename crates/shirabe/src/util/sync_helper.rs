//! ref: composer/src/Composer/Util/SyncHelper.php

use crate::downloader::DownloadManager;
use crate::downloader::DownloaderInterface;
use crate::package::PackageInterfaceHandle;
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
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        match self {
            Self::Interface(d) => d.download3(package, path, prev_package).await,
            Self::Manager(d) => d.borrow().download(package, path, prev_package).await,
        }
    }

    async fn prepare(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
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
        package: PackageInterfaceHandle,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        match self {
            Self::Interface(d) => d.install2(package, path).await,
            Self::Manager(d) => d.borrow().install(package, path).await,
        }
    }

    async fn update(
        &self,
        package: PackageInterfaceHandle,
        prev_package: PackageInterfaceHandle,
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
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
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
    pub fn download_and_install_package_sync(
        r#loop: &std::rc::Rc<std::cell::RefCell<Loop>>,
        downloader: DownloaderOrManager<'_>,
        path: String,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<()> {
        let r#type = if prev_package.is_some() {
            "update"
        } else {
            "install"
        };

        let result: Result<()> = (|| -> Result<()> {
            Self::r#await(
                r#loop,
                Some(Box::pin(async {
                    downloader
                        .download(package.clone(), &path, prev_package.clone())
                        .await
                        .map(|_| ())
                })),
            )?;
            Self::r#await(
                r#loop,
                Some(Box::pin(async {
                    downloader
                        .prepare(r#type, package.clone(), &path, prev_package.clone())
                        .await
                        .map(|_| ())
                })),
            )?;
            if r#type == "update" {
                if let Some(prev) = &prev_package {
                    Self::r#await(
                        r#loop,
                        Some(Box::pin(async {
                            downloader
                                .update(package.clone(), prev.clone(), &path)
                                .await
                                .map(|_| ())
                        })),
                    )?;
                }
            } else {
                Self::r#await(
                    r#loop,
                    Some(Box::pin(async {
                        downloader.install(package.clone(), &path).await.map(|_| ())
                    })),
                )?;
            }
            Ok(())
        })();

        if result.is_err() {
            Self::r#await(
                r#loop,
                Some(Box::pin(async {
                    downloader
                        .cleanup(r#type, package.clone(), &path, prev_package.clone())
                        .await
                        .map(|_| ())
                })),
            )?;
            return result;
        }

        Self::r#await(
            r#loop,
            Some(Box::pin(async {
                downloader
                    .cleanup(r#type, package.clone(), &path, prev_package.clone())
                    .await
                    .map(|_| ())
            })),
        )?;
        Ok(())
    }

    pub fn r#await(
        r#loop: &std::rc::Rc<std::cell::RefCell<Loop>>,
        promise: Option<std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + '_>>>,
    ) -> Result<()> {
        if let Some(promise) = promise {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(r#loop.borrow_mut().wait(vec![promise], None))?;
        }
        Ok(())
    }
}
