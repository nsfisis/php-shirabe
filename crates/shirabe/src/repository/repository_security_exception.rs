//! ref: composer/src/Composer/Repository/RepositorySecurityException.php

use shirabe_php_shim::Exception;

/// Thrown when a security problem, like a broken or missing signature
#[derive(Debug)]
pub struct RepositorySecurityException(pub Exception);

impl std::fmt::Display for RepositorySecurityException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for RepositorySecurityException {}
