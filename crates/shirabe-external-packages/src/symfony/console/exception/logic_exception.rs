//! ref: composer/vendor/symfony/console/Exception/LogicException.php

use super::exception_interface::ExceptionInterface;

#[derive(Debug)]
pub struct LogicException(pub shirabe_php_shim::LogicException);

impl std::fmt::Display for LogicException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for LogicException {}

impl ExceptionInterface for LogicException {}
