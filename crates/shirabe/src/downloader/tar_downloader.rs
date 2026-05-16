//! ref: composer/src/Composer/Downloader/TarDownloader.php

use crate::downloader::archive_downloader::ArchiveDownloader;
use crate::package::package_interface::PackageInterface;
use anyhow::Result;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::PharData;

#[derive(Debug)]
pub struct TarDownloader {
    inner: ArchiveDownloader,
}

impl TarDownloader {
    pub(crate) fn extract(
        &self,
        package: &dyn PackageInterface,
        file: &str,
        path: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        let archive = PharData::new(file.to_string());
        archive.extract_to(path, None, true);

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }
}
