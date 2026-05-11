//! ref: composer/src/Composer/Downloader/MaxFileSizeExceededException.php

use crate::downloader::transport_exception::TransportException;

pub struct MaxFileSizeExceededException(pub TransportException);
