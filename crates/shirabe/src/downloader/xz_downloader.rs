//! ref: composer/src/Composer/Downloader/XzDownloader.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::archive_downloader::ArchiveDownloader;
use crate::downloader::file_downloader::FileDownloader;
use crate::event_dispatcher::event_dispatcher::EventDispatcher;
use crate::io::io_interface::IOInterface;
use crate::package::package_interface::PackageInterface;
use crate::util::filesystem::Filesystem;
use crate::util::http_downloader::HttpDownloader;
use crate::util::process_executor::ProcessExecutor;
use anyhow::{Result, bail};
use indexmap::IndexMap;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct XzDownloader {
    inner: FileDownloader,
    cleanup_executed: IndexMap<String, bool>,
}

impl XzDownloader {
    pub fn new(
        io: Box<dyn IOInterface>,
        config: Config,
        http_downloader: HttpDownloader,
        event_dispatcher: Option<EventDispatcher>,
        cache: Option<Cache>,
        filesystem: Filesystem,
        process: ProcessExecutor,
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

    pub(crate) fn extract(
        &mut self,
        package: &dyn PackageInterface,
        file: &str,
        path: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        let command = vec!["tar", "-xJf", file, "-C", path];

        let mut ignored_output = PhpMixed::Null;
        if self.inner.process.execute(
            PhpMixed::List(
                command
                    .iter()
                    .map(|s| Box::new(PhpMixed::String(s.to_string())))
                    .collect(),
            ),
            Some(&mut ignored_output),
            None,
        )? == 0
        {
            return Ok(shirabe_external_packages::react::promise::resolve(None));
        }

        let process_error = format!(
            "Failed to execute {}\n\n{}",
            command.join(" "),
            self.inner.process.get_error_output()
        );

        bail!(process_error);
    }
}

impl crate::downloader::downloader_interface::DownloaderInterface for XzDownloader {
    fn get_installation_source(&self) -> String {
        self.inner.get_installation_source()
    }

    fn download(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
        output: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.inner.download(package, path, prev_package, output)
    }

    fn prepare(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.inner.prepare(r#type, package, path, prev_package)
    }

    fn install(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        output: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.inner.install(package, path, output)
    }

    fn update(
        &self,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
        path: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.inner.update(initial, target, path)
    }

    fn remove(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        output: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.inner.remove(package, path, output)
    }

    fn cleanup(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.inner.cleanup(r#type, package, path, prev_package)
    }
}
