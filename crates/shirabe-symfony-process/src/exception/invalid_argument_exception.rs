//! ref: composer/vendor/symfony/process/Exception/InvalidArgumentException.php

use bstr::BString;

#[derive(Debug)]
pub struct InvalidArgumentException {
    inner: shirabe_php_shim::InvalidArgumentException,
}

impl InvalidArgumentException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self {
            inner: shirabe_php_shim::InvalidArgumentException::new(message),
        }
    }
}

shirabe_php_shim::impl_php_exception!(
    InvalidArgumentException,
    inner,
    r"Symfony\Component\Process\Exception\InvalidArgumentException"
);
