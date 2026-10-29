//! ref: composer/src/Composer/Downloader/RarDownloader.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::{ArchiveDownloader, ChangeReportInterface, FileDownloader};
use crate::event_dispatcher::EventDispatcher;
use crate::io::IOInterface;
use crate::package::PackageInterfaceHandle;
use crate::util::{Filesystem, HttpDownloader, IniHelper, Platform, ProcessExecutor};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, RarArchive, RuntimeException, UnexpectedValueException, class_exists, impl_php_class,
    implode, path_to_bytes,
};

#[derive(Debug)]
pub struct RarDownloader {
    inner: FileDownloader,
    cleanup_executed: std::cell::RefCell<IndexMap<bstr::BString, bool>>,
}

impl_php_class!(RarDownloader, r"Composer\Downloader\RarDownloader");

impl RarDownloader {
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

impl ArchiveDownloader for RarDownloader {
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
        let mut process_error: Option<String> = None;

        if !Platform::is_windows() {
            let command: Vec<BString> = vec![
                "sh".into(),
                "-c".into(),
                r#"unrar x -- "$0" "$1" >/dev/null && chmod -R u+w "$1""#.into(),
                path_to_bytes(file).into(),
                path_to_bytes(path).into(),
            ];

            let mut process_output = PhpMixed::Null;
            if self
                .inner
                .process
                .borrow_mut()
                .execute(&command, &mut process_output, None)?
                == 0
            {
                return Ok(None);
            }

            process_error = Some(format!(
                "Failed to execute {}\n\n{}",
                implode(" ", &command).display_lossy(),
                self.inner
                    .process
                    .borrow()
                    .get_error_output()
                    .display_lossy(),
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
                    process_error.as_deref().unwrap_or_default(),
                )
            };
            return Err(RuntimeException::new(error).into());
        }

        let rar_archive = RarArchive::open(file);
        if rar_archive.is_none() {
            return Err(UnexpectedValueException::new(format!(
                "Could not open RAR archive: {}",
                file.display()
            ))
            .into());
        }
        let rar_archive = rar_archive.unwrap();

        let entries = rar_archive.get_entries();
        if entries.is_none() {
            return Err(RuntimeException::new(
                "Could not retrieve RAR archive entries".to_string(),
            )
            .into());
        }

        for entry in entries.unwrap() {
            if !entry.extract(path) {
                return Err(RuntimeException::new("Could not extract entry".to_string()).into());
            }
        }

        rar_archive.close();

        Ok(None)
    }
}

#[async_trait::async_trait(?Send)]
impl ChangeReportInterface for RarDownloader {
    async fn get_local_changes(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<bstr::BString>> {
        self.inner.base_get_local_changes(self, package, path).await
    }
}

#[async_trait::async_trait(?Send)]
impl crate::downloader::DownloaderInterface for RarDownloader {
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
