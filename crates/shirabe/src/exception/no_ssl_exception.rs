//! ref: composer/src/Composer/Exception/NoSslException.php

use shirabe_php_shim::RuntimeException;

/// Specific exception for Composer\Util\HttpDownloader creation.
#[derive(Debug)]
pub struct NoSslException(pub RuntimeException);

impl std::fmt::Display for NoSslException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for NoSslException {}
