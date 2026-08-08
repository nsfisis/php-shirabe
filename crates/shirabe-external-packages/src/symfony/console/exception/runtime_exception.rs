//! ref: composer/vendor/symfony/console/Exception/RuntimeException.php

use super::exception_interface::ExceptionInterface;

#[derive(Debug)]
pub struct RuntimeException(pub shirabe_php_shim::RuntimeException);

impl RuntimeException {
    pub fn new(message: String) -> Self {
        Self(shirabe_php_shim::RuntimeException::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(
    RuntimeException,
    0,
    r"Symfony\Component\Console\Exception\RuntimeException"
);

impl ExceptionInterface for RuntimeException {}
