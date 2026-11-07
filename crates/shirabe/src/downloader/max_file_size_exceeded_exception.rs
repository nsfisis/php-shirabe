//! ref: composer/src/Composer/Downloader/MaxFileSizeExceededException.php sha1:fb8a1707095551a801808290792a11d9eede273e

use crate::downloader::TransportException;
use bstr::BString;
use shirabe_php_shim::impl_php_exception;

#[derive(Debug)]
pub struct MaxFileSizeExceededException(pub TransportException);

impl MaxFileSizeExceededException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self(TransportException::new(message, 0))
    }
}

impl_php_exception!(
    MaxFileSizeExceededException,
    0,
    r"Composer\Downloader\MaxFileSizeExceededException"
);
