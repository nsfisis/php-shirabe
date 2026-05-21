//! ref: composer/src/Composer/Downloader/XzDownloader.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::ArchiveDownloader;
use crate::downloader::FileDownloader;
use crate::event_dispatcher::EventDispatcher;
use crate::io::IOInterface;
use crate::package::PackageInterface;
use crate::util::Filesystem;
use crate::util::HttpDownloader;
use crate::util::ProcessExecutor;
use anyhow::{Result, bail};
use indexmap::IndexMap;
use shirabe_external_packages::react::promise::PromiseInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct XzDownloader {
    inner: FileDownloader,
    cleanup_executed: IndexMap<String, bool>,
}

impl XzDownloader {
    pub fn new(
        io: Box<dyn IOInterface>,
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

    pub(crate) fn extract(
        &mut self,
        package: &dyn PackageInterface,
        file: &str,
        path: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
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
            return Ok(shirabe_external_packages::react::promise::resolve(None));
        }

        let process_error = format!(
            "Failed to execute {}\n\n{}",
            command.join(" "),
            self.inner.process.borrow().get_error_output()
        );

        bail!(process_error);
    }
}

impl crate::downloader::DownloaderInterface for XzDownloader {
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
