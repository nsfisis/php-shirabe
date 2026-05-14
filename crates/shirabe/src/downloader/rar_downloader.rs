//! ref: composer/src/Composer/Downloader/RarDownloader.php

use anyhow::Result;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::{class_exists, implode, RarArchive, RuntimeException, UnexpectedValueException};
use crate::downloader::archive_downloader::ArchiveDownloader;
use crate::package::package_interface::PackageInterface;
use crate::util::ini_helper::IniHelper;
use crate::util::platform::Platform;

pub struct RarDownloader {
    inner: ArchiveDownloader,
}

impl RarDownloader {
    pub(crate) fn extract(&self, _package: &dyn PackageInterface, file: &str, path: &str) -> Result<Box<dyn PromiseInterface>> {
        let mut process_error: Option<String> = None;

        if !Platform::is_windows() {
            let command = vec![
                "sh".to_string(),
                "-c".to_string(),
                r#"unrar x -- "$0" "$1" >/dev/null && chmod -R u+w "$1""#.to_string(),
                file.to_string(),
                path.to_string(),
            ];

            if self.inner.process.execute(&command, &mut String::new()) == 0 {
                return Ok(shirabe_external_packages::react::promise::resolve(None));
            }

            process_error = Some(format!(
                "Failed to execute {}\n\n{}",
                implode(" ", &command),
                self.inner.process.get_error_output(),
            ));
        }

        if !class_exists("RarArchive") {
            let ini_message = IniHelper::get_message();
            let error = if !Platform::is_windows() {
                format!("Could not decompress the archive, enable the PHP rar extension.\n{}", ini_message)
            } else {
                format!(
                    "Could not decompress the archive, enable the PHP rar extension or install unrar.\n{}\n{}",
                    ini_message,
                    process_error.as_deref().unwrap_or(""),
                )
            };
            return Err(RuntimeException { message: error, code: 0 }.into());
        }

        let rar_archive = RarArchive::open(file);
        if rar_archive.is_none() {
            return Err(UnexpectedValueException {
                message: format!("Could not open RAR archive: {}", file),
                code: 0,
            }.into());
        }
        let rar_archive = rar_archive.unwrap();

        let entries = rar_archive.get_entries();
        if entries.is_none() {
            return Err(RuntimeException {
                message: "Could not retrieve RAR archive entries".to_string(),
                code: 0,
            }.into());
        }

        for entry in entries.unwrap() {
            if !entry.extract(path) {
                return Err(RuntimeException {
                    message: "Could not extract entry".to_string(),
                    code: 0,
                }.into());
            }
        }

        rar_archive.close();

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }
}
