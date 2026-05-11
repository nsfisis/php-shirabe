//! ref: composer/src/Composer/Exception/NoSslException.php

use shirabe_php_shim::RuntimeException;

/// Specific exception for Composer\Util\HttpDownloader creation.
#[derive(Debug)]
pub struct NoSslException(pub RuntimeException);
