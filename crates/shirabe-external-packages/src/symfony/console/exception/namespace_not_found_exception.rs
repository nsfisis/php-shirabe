//! ref: composer/vendor/symfony/console/Exception/NamespaceNotFoundException.php

use super::command_not_found_exception::CommandNotFoundException;
use super::exception_interface::ExceptionInterface;

#[derive(Debug)]
pub struct NamespaceNotFoundException(pub CommandNotFoundException);

impl std::fmt::Display for NamespaceNotFoundException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for NamespaceNotFoundException {}

impl ExceptionInterface for NamespaceNotFoundException {}
