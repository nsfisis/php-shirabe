//! ref: composer/src/Composer/Repository/InvalidRepositoryException.php sha1:b8ff2a8c1fa78a139b74dc9c6677486b7f81a731

use bstr::BString;
use shirabe_php_shim::Exception;

/// Exception thrown when a package repository is utterly broken
#[derive(Debug)]
pub struct InvalidRepositoryException(pub Exception);

impl InvalidRepositoryException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self(Exception::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(
    InvalidRepositoryException,
    0,
    r"Composer\Repository\InvalidRepositoryException"
);
