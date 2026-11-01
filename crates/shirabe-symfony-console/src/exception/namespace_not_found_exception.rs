//! ref: composer/vendor/symfony/console/Exception/NamespaceNotFoundException.php

use super::command_not_found_exception::CommandNotFoundException;
use super::exception_interface::ExceptionInterface;
use bstr::BString;

#[derive(Debug)]
pub struct NamespaceNotFoundException(pub CommandNotFoundException);

impl NamespaceNotFoundException {
    #[track_caller]
    pub fn new(message: String, alternatives: Vec<BString>, code: i64) -> Self {
        Self(CommandNotFoundException::new(message, alternatives, code))
    }
}

shirabe_php_shim::impl_php_exception!(
    NamespaceNotFoundException,
    0,
    r"Symfony\Component\Console\Exception\NamespaceNotFoundException"
);

impl ExceptionInterface for NamespaceNotFoundException {}
