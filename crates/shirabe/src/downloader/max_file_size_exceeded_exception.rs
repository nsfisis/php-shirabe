//! ref: composer/src/Composer/Downloader/MaxFileSizeExceededException.php

use crate::downloader::transport_exception::TransportException;

#[derive(Debug)]
pub struct MaxFileSizeExceededException(pub TransportException);

impl MaxFileSizeExceededException {
    pub fn new(message: String) -> Self {
        Self(TransportException::new(message, 0))
    }
}

impl std::fmt::Display for MaxFileSizeExceededException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for MaxFileSizeExceededException {}
