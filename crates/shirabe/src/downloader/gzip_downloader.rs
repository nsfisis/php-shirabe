//! ref: composer/src/Composer/Downloader/GzipDownloader.php

use crate::downloader::archive_downloader::ArchiveDownloader;
use crate::package::package_interface::PackageInterface;
use crate::util::platform::Platform;
use anyhow::Result;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, PATHINFO_FILENAME, PHP_URL_PATH, RuntimeException, extension_loaded,
    fclose, fopen, fwrite, gzclose, gzopen, gzread, implode, parse_url, pathinfo, strtr,
};

pub struct GzipDownloader {
    inner: ArchiveDownloader,
}

impl GzipDownloader {
    pub(crate) fn extract(
        &self,
        package: &dyn PackageInterface,
        file: &str,
        path: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        let filename = pathinfo(
            parse_url(
                &strtr(&package.get_dist_url().unwrap_or_default(), "\\", "/"),
                PHP_URL_PATH,
            ),
            PATHINFO_FILENAME,
        );
        let target_filepath = format!("{}{}{}", path, DIRECTORY_SEPARATOR, filename);

        if !Platform::is_windows() {
            let command = vec![
                "sh".to_string(),
                "-c".to_string(),
                "gzip -cd -- \"$0\" > \"$1\"".to_string(),
                file.to_string(),
                target_filepath.clone(),
            ];

            if self.inner.process.execute(&command, &mut String::new()) == 0 {
                return Ok(shirabe_external_packages::react::promise::resolve(None));
            }

            if extension_loaded("zlib") {
                self.extract_using_ext(file, &target_filepath);
                return Ok(shirabe_external_packages::react::promise::resolve(None));
            }

            let process_error = format!(
                "Failed to execute {}\n\n{}",
                implode(" ", &command),
                self.inner.process.get_error_output(),
            );
            return Err(anyhow::anyhow!(RuntimeException {
                message: process_error,
                code: 0
            }));
        }

        self.extract_using_ext(file, &target_filepath);

        Ok(shirabe_external_packages::react::promise::resolve(None))
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
