//! ref: composer/vendor/symfony/console/Exception/InvalidArgumentException.php

use super::exception_interface::ExceptionInterface;

#[derive(Debug)]
pub struct InvalidArgumentException(pub shirabe_php_shim::InvalidArgumentException);

impl InvalidArgumentException {
    pub fn new(message: String) -> Self {
        Self(shirabe_php_shim::InvalidArgumentException::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(
    InvalidArgumentException,
    0,
    r"Symfony\Component\Console\Exception\InvalidArgumentException"
);

impl ExceptionInterface for InvalidArgumentException {}
