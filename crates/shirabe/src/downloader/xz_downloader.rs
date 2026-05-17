//! ref: composer/src/Composer/Downloader/XzDownloader.php

use crate::downloader::archive_downloader::ArchiveDownloader;
use crate::downloader::file_downloader::FileDownloader;
use crate::package::package_interface::PackageInterface;
use anyhow::{Result, bail};
use indexmap::IndexMap;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;

#[derive(Debug)]
pub struct XzDownloader {
    inner: FileDownloader,
    cleanup_executed: IndexMap<String, bool>,
}

impl XzDownloader {
    pub(crate) fn extract(
        &self,
        package: &dyn PackageInterface,
        file: &str,
        path: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        let command = vec!["tar", "-xJf", file, "-C", path];

        let mut ignored_output = String::new();
        if self.inner.process.execute(&command, &mut ignored_output) == 0 {
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
