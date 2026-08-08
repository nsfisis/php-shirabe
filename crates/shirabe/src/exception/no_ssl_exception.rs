//! ref: composer/src/Composer/Exception/NoSslException.php

use shirabe_php_shim::RuntimeException;

/// Specific exception for Composer\Util\HttpDownloader creation.
#[derive(Debug)]
pub struct NoSslException(pub RuntimeException);

impl NoSslException {
    pub fn new(message: String) -> Self {
        Self(RuntimeException::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(NoSslException, 0, r"Composer\Exception\NoSslException");
