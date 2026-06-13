//! ref: composer/vendor/symfony/console/Exception/MissingInputException.php

use super::exception_interface::ExceptionInterface;
use super::runtime_exception::RuntimeException;

#[derive(Debug)]
pub struct MissingInputException(pub RuntimeException);

impl std::fmt::Display for MissingInputException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MissingInputException {}

impl ExceptionInterface for MissingInputException {}
