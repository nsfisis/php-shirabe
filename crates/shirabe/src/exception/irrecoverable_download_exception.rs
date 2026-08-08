//! ref: composer/src/Composer/Exception/IrrecoverableDownloadException.php

use shirabe_php_shim::RuntimeException;

#[derive(Debug)]
pub struct IrrecoverableDownloadException(pub RuntimeException);

impl IrrecoverableDownloadException {
    pub fn new(message: String) -> Self {
        Self(RuntimeException::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(
    IrrecoverableDownloadException,
    0,
    r"Composer\Exception\IrrecoverableDownloadException"
);
