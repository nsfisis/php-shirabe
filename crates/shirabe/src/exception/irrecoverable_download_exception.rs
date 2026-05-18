//! ref: composer/src/Composer/Exception/IrrecoverableDownloadException.php

use shirabe_php_shim::RuntimeException;

#[derive(Debug)]
pub struct IrrecoverableDownloadException(pub RuntimeException);

impl std::fmt::Display for IrrecoverableDownloadException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for IrrecoverableDownloadException {}
