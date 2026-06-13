//! ref: composer/vendor/symfony/console/Exception/InvalidArgumentException.php

use super::exception_interface::ExceptionInterface;

#[derive(Debug)]
pub struct InvalidArgumentException(pub shirabe_php_shim::InvalidArgumentException);

impl std::fmt::Display for InvalidArgumentException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for InvalidArgumentException {}

impl ExceptionInterface for InvalidArgumentException {}
