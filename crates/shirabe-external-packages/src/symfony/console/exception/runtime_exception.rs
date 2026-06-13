//! ref: composer/vendor/symfony/console/Exception/RuntimeException.php

use super::exception_interface::ExceptionInterface;

#[derive(Debug)]
pub struct RuntimeException(pub shirabe_php_shim::RuntimeException);

impl std::fmt::Display for RuntimeException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for RuntimeException {}

impl ExceptionInterface for RuntimeException {}
