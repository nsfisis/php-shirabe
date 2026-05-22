//! ref: composer/src/Composer/Downloader/GzipDownloader.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::ArchiveDownloader;
use crate::downloader::FileDownloader;
use crate::event_dispatcher::EventDispatcher;
use crate::io::IOInterface;
use crate::package::PackageInterface;
use crate::util::Filesystem;
use crate::util::HttpDownloader;
use crate::util::Platform;
use crate::util::ProcessExecutor;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, PATHINFO_FILENAME, PHP_URL_PATH, PhpMixed, RuntimeException,
    extension_loaded, fclose, fopen, fwrite, gzclose, gzopen, gzread, implode, parse_url, pathinfo,
    strtr,
};

#[derive(Debug)]
pub struct GzipDownloader {
    inner: FileDownloader,
    cleanup_executed: IndexMap<String, bool>,
}

impl GzipDownloader {
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
        package: &dyn PackageInterface,
        file: &str,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        let filename = pathinfo(
            parse_url(
                &strtr(&package.get_dist_url().unwrap_or_default(), "\\", "/"),
                PHP_URL_PATH,
            ),
            PATHINFO_FILENAME,
        );
        let target_filepath = format!(
            "{}{}{}",
            path,
            DIRECTORY_SEPARATOR,
            filename.as_string().unwrap_or_default()
        );

        if !Platform::is_windows() {
            let command = vec![
                "sh".to_string(),
                "-c".to_string(),
                "gzip -cd -- \"$0\" > \"$1\"".to_string(),
                file.to_string(),
                target_filepath.clone(),
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

            if extension_loaded("zlib") {
                self.extract_using_ext(file, &target_filepath);
                return Ok(None);
            }

            let process_error = format!(
                "Failed to execute {}\n\n{}",
                implode(" ", &command),
                self.inner.process.borrow().get_error_output(),
            );
            return Err(anyhow::anyhow!(RuntimeException {
                message: process_error,
                code: 0
            }));
        }

        self.extract_using_ext(file, &target_filepath);

        Ok(None)
    }

    fn extract_using_ext(&self, file: &str, target_filepath: &str) {
        let archive_file = gzopen(file, "rb");
        let target_file = fopen(target_filepath, "wb");
        loop {
            let string = gzread(archive_file.clone(), 4096);
            if string.is_empty() {
                break;
            }
            fwrite(target_file.clone(), &string, Platform::strlen(&string));
        }
        gzclose(archive_file);
        fclose(target_file);
    }
}

impl crate::downloader::DownloaderInterface for GzipDownloader {
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
