//! ref: composer/vendor/symfony/console/Exception/MissingInputException.php

use super::exception_interface::ExceptionInterface;
use super::runtime_exception::RuntimeException;

#[derive(Debug)]
pub struct MissingInputException(pub RuntimeException);

impl MissingInputException {
    pub fn new(message: String) -> Self {
        Self(RuntimeException::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(
    MissingInputException,
    0,
    r"Symfony\Component\Console\Exception\MissingInputException"
);

impl ExceptionInterface for MissingInputException {}
