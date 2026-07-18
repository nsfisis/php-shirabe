//! ref: composer/src/Composer/Downloader/XzDownloader.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::ArchiveDownloader;
use crate::downloader::ChangeReportInterface;
use crate::downloader::FileDownloader;
use crate::event_dispatcher::EventDispatcher;
use crate::io::IOInterface;
use crate::package::PackageInterfaceHandle;
use crate::util::Filesystem;
use crate::util::HttpDownloader;
use crate::util::ProcessExecutor;
use anyhow::bail;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct XzDownloader {
    inner: FileDownloader,
    cleanup_executed: std::cell::RefCell<IndexMap<String, bool>>,
}

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

    fn cleanup_executed(&self) -> &std::cell::RefCell<IndexMap<String, bool>> {
        &self.cleanup_executed
    }

    async fn extract(
        &self,
        _package: PackageInterfaceHandle,
        file: &str,
        path: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let command = ["tar", "-xJf", file, "-C", path];

        let mut ignored_output = PhpMixed::Null;
        if self.inner.process.borrow_mut().execute(
            PhpMixed::List(
                command
                    .iter()
                    .map(|s| PhpMixed::String(s.to_string()))
                    .collect(),
            ),
            &mut ignored_output,
            None,
        )? == 0
        {
            return Ok(None);
        }

        let process_error = format!(
            "Failed to execute {}\n\n{}",
            command.join(" "),
            self.inner.process.borrow().get_error_output()
        );

        bail!(process_error);
    }
}

impl ChangeReportInterface for XzDownloader {
    fn get_local_changes(
        &self,
        package: PackageInterfaceHandle,
        path: &str,
    ) -> anyhow::Result<Option<String>> {
        self.inner.get_local_changes(package, path)
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
        path: &str,
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
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as ArchiveDownloader>::prepare(self, r#type, package, path, prev_package).await
    }

    async fn install(
        &self,
        package: PackageInterfaceHandle,
        path: &str,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as ArchiveDownloader>::install(self, package, path, output).await
    }

    async fn update(
        &self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.inner.update(initial, target, path).await
    }

    async fn remove(
        &self,
        package: PackageInterfaceHandle,
        path: &str,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.inner.remove(package, path, output).await
    }

    async fn cleanup(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as ArchiveDownloader>::cleanup(self, r#type, package, path, prev_package).await
    }
}
