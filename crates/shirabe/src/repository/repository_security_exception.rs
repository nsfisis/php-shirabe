//! ref: composer/src/Composer/Repository/RepositorySecurityException.php

use shirabe_php_shim::Exception;

/// Thrown when a security problem, like a broken or missing signature
#[derive(Debug)]
pub struct RepositorySecurityException(pub Exception);

impl RepositorySecurityException {
    pub fn new(message: String) -> Self {
        Self(Exception::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(
    RepositorySecurityException,
    0,
    r"Composer\Repository\RepositorySecurityException"
);
