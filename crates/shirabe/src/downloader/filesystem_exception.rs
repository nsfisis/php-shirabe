//! ref: composer/src/Composer/Downloader/FilesystemException.php

use shirabe_php_shim::Exception;

#[derive(Debug)]
pub struct FilesystemException(pub Exception);

impl FilesystemException {
    pub fn new(message: String, code: i64) -> Self {
        FilesystemException(Exception {
            message: format!("Filesystem exception: \n{}", message),
            code,
        })
    }
}

impl std::fmt::Display for FilesystemException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for FilesystemException {}
