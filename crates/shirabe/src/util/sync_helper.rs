//! ref: composer/src/Composer/Util/SyncHelper.php sha1:113bfee35e9ccac03bf1b90b38b702f684186ac9

use crate::downloader::{DownloadManagerInterface, DownloaderInterface};
use crate::package::PackageInterfaceHandle;
use crate::util::r#loop::Loop;
use bstr::BStr;
use shirabe_php_shim::PhpMixed;

pub enum DownloaderOrManager<'a> {
    Interface(&'a std::rc::Rc<std::cell::RefCell<dyn DownloaderInterface>>),
    Manager(&'a std::rc::Rc<std::cell::RefCell<dyn DownloadManagerInterface>>),
}

impl<'a> DownloaderOrManager<'a> {
    async fn download(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        match self {
            Self::Interface(d) => d.borrow_mut().download(package, path, prev_package).await,
            Self::Manager(d) => d.borrow().download(package, path, prev_package).await,
        }
    }

    async fn prepare(
        &self,
        r#type: &BStr,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        match self {
            Self::Interface(d) => {
                d.borrow_mut()
                    .prepare(r#type, package, path, prev_package)
                    .await
            }
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
        path: &std::path::Path,
    ) -> anyhow::Result<Option<PhpMixed>> {
        match self {
            Self::Interface(d) => d.borrow_mut().install(package, path).await,
            Self::Manager(d) => d.borrow().install(package, path).await,
        }
    }

    async fn update(
        &self,
        package: PackageInterfaceHandle,
        prev_package: PackageInterfaceHandle,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<PhpMixed>> {
        match self {
            Self::Interface(d) => d.borrow_mut().update(package, prev_package, path).await,
            Self::Manager(d) => d.borrow().update(package, prev_package, path).await,
        }
    }

    async fn cleanup(
        &self,
        r#type: &BStr,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        match self {
            Self::Interface(d) => {
                d.borrow_mut()
                    .cleanup(r#type, package, path, prev_package)
                    .await
            }
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
    pub async fn download_and_install_package_sync(
        r#loop: &std::rc::Rc<std::cell::RefCell<Loop>>,
        downloader: DownloaderOrManager<'_>,
        path: std::path::PathBuf,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<()> {
        let r#type = if prev_package.is_some() {
            BStr::new("update")
        } else {
            BStr::new("install")
        };

        let result: anyhow::Result<()> = async {
            Self::r#await(
                r#loop,
                Some(Box::pin(async {
                    downloader
                        .download(package.clone(), &path, prev_package.clone())
                        .await
                        .map(|_| ())
                })),
            )
            .await?;
            Self::r#await(
                r#loop,
                Some(Box::pin(async {
                    downloader
                        .prepare(r#type, package.clone(), &path, prev_package.clone())
                        .await
                        .map(|_| ())
                })),
            )
            .await?;
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
                    )
                    .await?;
                }
            } else {
                Self::r#await(
                    r#loop,
                    Some(Box::pin(async {
                        downloader.install(package.clone(), &path).await.map(|_| ())
                    })),
                )
                .await?;
            }
            Ok(())
        }
        .await;

        if result.is_err() {
            Self::r#await(
                r#loop,
                Some(Box::pin(async {
                    downloader
                        .cleanup(r#type, package.clone(), &path, prev_package.clone())
                        .await
                        .map(|_| ())
                })),
            )
            .await?;
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
        )
        .await?;
        Ok(())
    }

    pub async fn r#await(
        r#loop: &std::rc::Rc<std::cell::RefCell<Loop>>,
        promise: Option<
            std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + '_>>,
        >,
    ) -> anyhow::Result<()> {
        if let Some(promise) = promise {
            r#loop.borrow().wait(vec![promise], None).await?;
        }
        Ok(())
    }
}
