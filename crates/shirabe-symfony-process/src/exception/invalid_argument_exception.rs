//! ref: composer/vendor/symfony/process/Exception/InvalidArgumentException.php

#[derive(Debug)]
pub struct InvalidArgumentException {
    inner: shirabe_php_shim::InvalidArgumentException,
}

impl InvalidArgumentException {
    pub fn new(message: String) -> Self {
        Self {
            inner: shirabe_php_shim::InvalidArgumentException::new(message),
        }
    }
}

shirabe_php_shim::impl_php_exception!(
    InvalidArgumentException,
    inner,
    r"Symfony\Component\Process\Exception\InvalidArgumentException"
);
