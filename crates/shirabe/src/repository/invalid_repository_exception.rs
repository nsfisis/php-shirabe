//! ref: composer/src/Composer/Repository/InvalidRepositoryException.php

use shirabe_php_shim::Exception;

/// Exception thrown when a package repository is utterly broken
#[derive(Debug)]
pub struct InvalidRepositoryException(pub Exception);
