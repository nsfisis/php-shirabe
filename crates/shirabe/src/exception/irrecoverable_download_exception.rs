//! ref: composer/src/Composer/Exception/IrrecoverableDownloadException.php

use bstr::BString;
use shirabe_php_shim::RuntimeException;

#[derive(Debug)]
pub struct IrrecoverableDownloadException(pub RuntimeException);

impl IrrecoverableDownloadException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self(RuntimeException::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(
    IrrecoverableDownloadException,
    0,
    r"Composer\Exception\IrrecoverableDownloadException"
);
