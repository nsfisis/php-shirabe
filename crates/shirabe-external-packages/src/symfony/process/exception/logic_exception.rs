//! ref: composer/vendor/symfony/process/Exception/LogicException.php

#[derive(Debug)]
pub struct LogicException {
    inner: shirabe_php_shim::LogicException,
}

impl LogicException {
    pub fn new(message: String) -> Self {
        Self {
            inner: shirabe_php_shim::LogicException::new(message),
        }
    }
}

shirabe_php_shim::impl_php_exception!(
    LogicException,
    inner,
    r"Symfony\Component\Process\Exception\LogicException"
);
