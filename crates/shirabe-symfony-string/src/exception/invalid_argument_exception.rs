//! ref: composer/vendor/symfony/string/Exception/InvalidArgumentException.php

use super::exception_interface::ExceptionInterface;
use bstr::BString;

#[derive(Debug)]
pub struct InvalidArgumentException(pub shirabe_php_shim::InvalidArgumentException);

impl InvalidArgumentException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self(shirabe_php_shim::InvalidArgumentException::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(
    InvalidArgumentException,
    0,
    r"Symfony\Component\String\Exception\InvalidArgumentException"
);

impl ExceptionInterface for InvalidArgumentException {}
