//! ref: composer/vendor/symfony/console/Exception/LogicException.php

use super::exception_interface::ExceptionInterface;

#[derive(Debug)]
pub struct LogicException(pub shirabe_php_shim::LogicException);

impl LogicException {
    pub fn new(message: String) -> Self {
        Self(shirabe_php_shim::LogicException::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(
    LogicException,
    0,
    r"Symfony\Component\Console\Exception\LogicException"
);

impl ExceptionInterface for LogicException {}
