//! ref: composer/src/Composer/Repository/RepositorySecurityException.php

use shirabe_php_shim::Exception;

/// Thrown when a security problem, like a broken or missing signature
#[derive(Debug)]
pub struct RepositorySecurityException(pub Exception);
