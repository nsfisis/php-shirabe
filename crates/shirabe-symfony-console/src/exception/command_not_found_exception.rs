//! ref: composer/vendor/symfony/console/Exception/CommandNotFoundException.php

use super::exception_interface::ExceptionInterface;

#[derive(Debug)]
pub struct CommandNotFoundException {
    inner: shirabe_php_shim::InvalidArgumentException,
    alternatives: Vec<String>,
}

impl CommandNotFoundException {
    pub fn new(message: String, alternatives: Vec<String>, code: i64) -> Self {
        Self {
            inner: shirabe_php_shim::InvalidArgumentException::with_code(message, code),
            alternatives,
        }
    }

    pub fn get_alternatives(&self) -> &Vec<String> {
        &self.alternatives
    }
}

shirabe_php_shim::impl_php_exception!(
    CommandNotFoundException,
    inner,
    r"Symfony\Component\Console\Exception\CommandNotFoundException"
);

impl ExceptionInterface for CommandNotFoundException {}
