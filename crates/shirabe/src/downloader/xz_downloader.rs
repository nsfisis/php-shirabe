//! ref: composer/src/Composer/Downloader/XzDownloader.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::{ArchiveDownloader, ChangeReportInterface, FileDownloader};
use crate::event_dispatcher::EventDispatcher;
use crate::io::IOInterface;
use crate::package::PackageInterfaceHandle;
use crate::util::{Filesystem, HttpDownloader, ProcessExecutor};
use anyhow::bail;
use indexmap::IndexMap;
use shirabe_php_shim::{PhpMixed, impl_php_class, implode, path_to_bytes};

#[derive(Debug)]
pub struct XzDownloader {
    inner: FileDownloader,
    cleanup_executed: std::cell::RefCell<IndexMap<bstr::BString, bool>>,
}

impl_php_class!(XzDownloader, r"Composer\Downloader\XzDownloader");

impl XzDownloader {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
        event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
        cache: Option<std::rc::Rc<std::cell::RefCell<Cache>>>,
        filesystem: std::rc::Rc<std::cell::RefCell<Filesystem>>,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> Self {
        Self {
            inner: FileDownloader::new(
                io,
                config,
                http_downloader,
                event_dispatcher,
                cache,
                Some(filesystem),
                Some(process),
            ),
            cleanup_executed: std::cell::RefCell::new(IndexMap::new()),
        }
    }
}

impl ArchiveDownloader for XzDownloader {
    fn inner(&self) -> &FileDownloader {
        &self.inner
    }

    fn cleanup_executed(&self) -> &std::cell::RefCell<IndexMap<bstr::BString, bool>> {
        &self.cleanup_executed
    }

    async fn extract(
        &self,
        _package: PackageInterfaceHandle,
        file: &std::path::Path,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let command: Vec<bstr::BString> = vec![
            "tar".into(),
            "-xJf".into(),
            path_to_bytes(file).into(),
            "-C".into(),
            path_to_bytes(path).into(),
        ];

        let mut ignored_output = PhpMixed::Null;
        if self
            .inner
            .process
            .borrow_mut()
            .execute(&command, &mut ignored_output, None)?
            == 0
        {
            return Ok(None);
        }

        let process_error = format!(
            "Failed to execute {}\n\n{}",
            implode(" ", &command).display_lossy(),
            self.inner
                .process
                .borrow()
                .get_error_output()
                .display_lossy()
        );

        bail!(process_error);
    }
}

#[async_trait::async_trait(?Send)]
impl ChangeReportInterface for XzDownloader {
    async fn get_local_changes(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<bstr::BString>> {
        self.inner.base_get_local_changes(self, package, path).await
    }
}

#[async_trait::async_trait(?Send)]
impl crate::downloader::DownloaderInterface for XzDownloader {
    fn get_installation_source(&self) -> String {
        self.inner.get_installation_source()
    }

    fn as_change_report_interface(&self) -> Option<&dyn crate::downloader::ChangeReportInterface> {
        Some(self)
    }

    async fn download(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        prev_package: Option<PackageInterfaceHandle>,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.inner
            .download(package, path, prev_package, output)
            .await
    }

    async fn prepare(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as ArchiveDownloader>::prepare(self, r#type, package, path, prev_package).await
    }

    async fn install(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as ArchiveDownloader>::install(self, package, path, output).await
    }

    async fn update(
        &self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let appendix = self.get_install_operation_appendix(target.clone(), path);
        self.inner
            .base_update(self, appendix, initial, target, path)
            .await
    }

    async fn remove(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.inner.remove(package, path, output).await
    }

    async fn cleanup(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as ArchiveDownloader>::cleanup(self, r#type, package, path, prev_package).await
    }
}
