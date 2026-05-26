//! ref: composer/src/Composer/Downloader/TarDownloader.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::ArchiveDownloader;
use crate::downloader::DownloaderInterface;
use crate::downloader::FileDownloader;
use crate::event_dispatcher::EventDispatcher;
use crate::io::IOInterface;
use crate::package::PackageInterface;
use crate::util::Filesystem;
use crate::util::HttpDownloader;
use crate::util::ProcessExecutor;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{PharData, PhpMixed};

#[derive(Debug)]
pub struct TarDownloader {
    inner: FileDownloader,
    cleanup_executed: IndexMap<String, bool>,
}

impl TarDownloader {
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

    pub(crate) async fn extract(
        &self,
        package: &dyn PackageInterface,
        file: &str,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        let archive = PharData::new(file.to_string());
        archive.extract_to(path, None, true);

        Ok(None)
    }
}

#[async_trait::async_trait(?Send)]
impl DownloaderInterface for TarDownloader {
    fn get_installation_source(&self) -> String {
        self.inner.get_installation_source()
    }

    async fn download(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
        output: bool,
    ) -> Result<Option<PhpMixed>> {
        self.inner
            .download(package, path, prev_package, output)
            .await
    }

    async fn prepare(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<PhpMixed>> {
        self.inner
            .prepare(r#type, package, path, prev_package)
            .await
    }

    async fn install(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        output: bool,
    ) -> Result<Option<PhpMixed>> {
        self.inner.install(package, path, output).await
    }

    async fn update(
        &self,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        self.inner.update(initial, target, path).await
    }

    async fn remove(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        output: bool,
    ) -> Result<Option<PhpMixed>> {
        self.inner.remove(package, path, output).await
    }

    async fn cleanup(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<PhpMixed>> {
        self.inner
            .cleanup(r#type, package, path, prev_package)
            .await
    }
}
