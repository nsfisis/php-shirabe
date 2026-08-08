//! ref: composer/vendor/symfony/console/Exception/InvalidOptionException.php

use super::exception_interface::ExceptionInterface;
use shirabe_php_shim::InvalidArgumentException;

#[derive(Debug)]
pub struct InvalidOptionException(pub InvalidArgumentException);

impl InvalidOptionException {
    pub fn new(message: String) -> Self {
        Self(InvalidArgumentException::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(
    InvalidOptionException,
    0,
    r"Symfony\Component\Console\Exception\InvalidOptionException"
);

impl ExceptionInterface for InvalidOptionException {}
