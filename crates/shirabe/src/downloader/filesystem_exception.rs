//! ref: composer/src/Composer/Downloader/FilesystemException.php sha1:bba6413089b969fe05dc64513efa5ef997d756a9

use bstr::BString;
use shirabe_php_shim::{Exception, bconcat};

#[derive(Debug)]
pub struct FilesystemException(pub Exception);

impl FilesystemException {
    #[track_caller]
    pub fn new(message: impl Into<BString>, code: i64) -> Self {
        FilesystemException(Exception::with_code(
            bconcat!("Filesystem exception: \n", message.into()),
            code,
        ))
    }
}

shirabe_php_shim::impl_php_exception!(
    FilesystemException,
    0,
    r"Composer\Downloader\FilesystemException"
);
