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
use anyhow::{Result, bail};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct XzDownloader {
    inner: FileDownloader,
    cleanup_executed: IndexMap<String, bool>,
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
            cleanup_executed: IndexMap::new(),
        }
    }
}

impl ArchiveDownloader for XzDownloader {
    fn inner(&self) -> &FileDownloader {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut FileDownloader {
        &mut self.inner
    }

    fn cleanup_executed(&self) -> &IndexMap<String, bool> {
        &self.cleanup_executed
    }

    fn cleanup_executed_mut(&mut self) -> &mut IndexMap<String, bool> {
        &mut self.cleanup_executed
    }

    async fn extract(
        &mut self,
        _package: PackageInterfaceHandle,
        file: &str,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        let command = vec!["tar", "-xJf", file, "-C", path];

        let mut ignored_output = PhpMixed::Null;
        if self.inner.process.borrow_mut().execute(
            PhpMixed::List(
                command
                    .iter()
                    .map(|s| Box::new(PhpMixed::String(s.to_string())))
                    .collect(),
            ),
            Some(&mut ignored_output),
            (),
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
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
    ) -> Result<Option<String>> {
        self.inner.get_local_changes(package, path)
    }
}

#[async_trait::async_trait(?Send)]
impl crate::downloader::DownloaderInterface for XzDownloader {
    fn get_installation_source(&self) -> String {
        self.inner.get_installation_source()
    }

    fn as_change_report_interface(
        &mut self,
    ) -> Option<&mut dyn crate::downloader::ChangeReportInterface> {
        Some(self)
    }

    async fn download(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
        output: bool,
    ) -> Result<Option<PhpMixed>> {
        self.inner
            .download(package, path, prev_package, output)
            .await
    }

    async fn prepare(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        <Self as ArchiveDownloader>::prepare(self, r#type, package, path, prev_package).await
    }

    async fn install(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        output: bool,
    ) -> Result<Option<PhpMixed>> {
        <Self as ArchiveDownloader>::install(self, package, path, output).await
    }

    async fn update(
        &mut self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        self.inner.update(initial, target, path).await
    }

    async fn remove(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        output: bool,
    ) -> Result<Option<PhpMixed>> {
        self.inner.remove(package, path, output).await
    }

    async fn cleanup(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        <Self as ArchiveDownloader>::cleanup(self, r#type, package, path, prev_package).await
    }
}
