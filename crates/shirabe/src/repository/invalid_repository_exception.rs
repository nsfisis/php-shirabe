//! ref: composer/src/Composer/Repository/InvalidRepositoryException.php

use shirabe_php_shim::Exception;

/// Exception thrown when a package repository is utterly broken
#[derive(Debug)]
pub struct InvalidRepositoryException(pub Exception);

impl InvalidRepositoryException {
    pub fn new(message: String) -> Self {
        Self(Exception::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(
    InvalidRepositoryException,
    0,
    r"Composer\Repository\InvalidRepositoryException"
);
