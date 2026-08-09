//! ref: composer/vendor/symfony/process/Exception/RuntimeException.php

#[derive(Debug)]
pub struct RuntimeException {
    inner: shirabe_php_shim::RuntimeException,
}

impl RuntimeException {
    pub fn new(message: String) -> Self {
        Self {
            inner: shirabe_php_shim::RuntimeException::new(message),
        }
    }
}

shirabe_php_shim::impl_php_exception!(
    RuntimeException,
    inner,
    r"Symfony\Component\Process\Exception\RuntimeException"
);
