//! ref: composer/src/Composer/Downloader/MaxFileSizeExceededException.php

use crate::downloader::transport_exception::TransportException;
use bstr::BString;

#[derive(Debug)]
pub struct MaxFileSizeExceededException(pub TransportException);

impl MaxFileSizeExceededException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self(TransportException::new(message, 0))
    }
}

shirabe_php_shim::impl_php_exception!(
    MaxFileSizeExceededException,
    0,
    r"Composer\Downloader\MaxFileSizeExceededException"
);
