//! ref: composer/src/Composer/Downloader/PharDownloader.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::archive_downloader::ArchiveDownloader;
use crate::downloader::downloader_interface::DownloaderInterface;
use crate::downloader::file_downloader::FileDownloader;
use crate::event_dispatcher::event_dispatcher::EventDispatcher;
use crate::io::io_interface::IOInterface;
use crate::package::package_interface::PackageInterface;
use crate::util::filesystem::Filesystem;
use crate::util::http_downloader::HttpDownloader;
use crate::util::process_executor::ProcessExecutor;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::Phar;

#[derive(Debug)]
pub struct PharDownloader {
    inner: FileDownloader,
    cleanup_executed: IndexMap<String, bool>,
}

impl PharDownloader {
    pub fn new(
        io: Box<dyn IOInterface>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
        event_dispatcher: Option<EventDispatcher>,
        cache: Option<Cache>,
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

    pub(crate) fn extract(
        &self,
        package: &dyn PackageInterface,
        file: &str,
        path: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        // Can throw an UnexpectedValueException
        let archive = Phar::new(file.to_string());
        archive.extract_to(path, None, true);
        // TODO: handle openssl signed phars
        // https://github.com/composer/composer/pull/33#issuecomment-2250768
        // https://github.com/koto/phar-util
        // http://blog.kotowicz.net/2010/08/hardening-php-how-to-securely-include.html

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }
}

impl DownloaderInterface for PharDownloader {
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
