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
