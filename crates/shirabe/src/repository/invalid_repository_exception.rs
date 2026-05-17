//! ref: composer/src/Composer/Repository/InvalidRepositoryException.php

use shirabe_php_shim::Exception;

/// Exception thrown when a package repository is utterly broken
#[derive(Debug)]
pub struct InvalidRepositoryException(pub Exception);

impl std::fmt::Display for InvalidRepositoryException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for InvalidRepositoryException {}
