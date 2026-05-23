//! ref: composer/src/Composer/Downloader/RarDownloader.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::ArchiveDownloader;
use crate::downloader::FileDownloader;
use crate::event_dispatcher::EventDispatcher;
use crate::io::IOInterface;
use crate::package::PackageInterface;
use crate::util::Filesystem;
use crate::util::HttpDownloader;
use crate::util::IniHelper;
use crate::util::Platform;
use crate::util::ProcessExecutor;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, RarArchive, RuntimeException, UnexpectedValueException, class_exists, implode,
};

#[derive(Debug)]
pub struct RarDownloader {
    inner: FileDownloader,
    cleanup_executed: IndexMap<String, bool>,
}

impl RarDownloader {
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

    pub(crate) async fn extract(
        &mut self,
        _package: &dyn PackageInterface,
        file: &str,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        let mut process_error: Option<String> = None;

        if !Platform::is_windows() {
            let command = vec![
                "sh".to_string(),
                "-c".to_string(),
                r#"unrar x -- "$0" "$1" >/dev/null && chmod -R u+w "$1""#.to_string(),
                file.to_string(),
                path.to_string(),
            ];

            let mut process_output = PhpMixed::Null;
            if self.inner.process.borrow_mut().execute(
                PhpMixed::List(
                    command
                        .iter()
                        .map(|s| Box::new(PhpMixed::String(s.clone())))
                        .collect(),
                ),
                Some(&mut process_output),
                (),
            )? == 0
            {
                return Ok(None);
            }

            process_error = Some(format!(
                "Failed to execute {}\n\n{}",
                implode(" ", &command),
                self.inner.process.borrow().get_error_output(),
            ));
        }

        if !class_exists("RarArchive") {
            let ini_message = IniHelper::get_message();
            let error = if !Platform::is_windows() {
                format!(
                    "Could not decompress the archive, enable the PHP rar extension.\n{}",
                    ini_message
                )
            } else {
                format!(
                    "Could not decompress the archive, enable the PHP rar extension or install unrar.\n{}\n{}",
                    ini_message,
                    process_error.as_deref().unwrap_or(""),
                )
            };
            return Err(RuntimeException {
                message: error,
                code: 0,
            }
            .into());
        }

        let rar_archive = RarArchive::open(file);
        if rar_archive.is_none() {
            return Err(UnexpectedValueException {
                message: format!("Could not open RAR archive: {}", file),
                code: 0,
            }
            .into());
        }
        let rar_archive = rar_archive.unwrap();

        let entries = rar_archive.get_entries();
        if entries.is_none() {
            return Err(RuntimeException {
                message: "Could not retrieve RAR archive entries".to_string(),
                code: 0,
            }
            .into());
        }

        for entry in entries.unwrap() {
            if !entry.extract(path) {
                return Err(RuntimeException {
                    message: "Could not extract entry".to_string(),
                    code: 0,
                }
                .into());
            }
        }

        rar_archive.close();

        Ok(None)
    }
}

#[async_trait::async_trait(?Send)]
impl crate::downloader::DownloaderInterface for RarDownloader {
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
