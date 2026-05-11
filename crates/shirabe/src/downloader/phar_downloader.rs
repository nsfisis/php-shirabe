//! ref: composer/src/Composer/Downloader/PharDownloader.php

use anyhow::Result;
use shirabe_php_shim::Phar;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use crate::downloader::archive_downloader::ArchiveDownloader;
use crate::package::package_interface::PackageInterface;

#[derive(Debug)]
pub struct PharDownloader {
    inner: ArchiveDownloader,
}

impl PharDownloader {
    pub(crate) fn extract(&self, package: &dyn PackageInterface, file: &str, path: &str) -> Result<Box<dyn PromiseInterface>> {
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
