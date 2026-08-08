//! ref: composer/src/Composer/Downloader/FilesystemException.php

use shirabe_php_shim::Exception;

#[derive(Debug)]
pub struct FilesystemException(pub Exception);

impl FilesystemException {
    pub fn new(message: String, code: i64) -> Self {
        FilesystemException(Exception::with_code(
            format!("Filesystem exception: \n{}", message),
            code,
        ))
    }
}

shirabe_php_shim::impl_php_exception!(
    FilesystemException,
    0,
    r"Composer\Downloader\FilesystemException"
);
