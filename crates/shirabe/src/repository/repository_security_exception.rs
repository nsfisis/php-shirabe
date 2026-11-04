//! ref: composer/src/Composer/Repository/RepositorySecurityException.php sha1:25f7fbbd8391156425356cc42409d7206f2c4a97

use bstr::BString;
use shirabe_php_shim::Exception;

/// Thrown when a security problem, like a broken or missing signature
#[derive(Debug)]
pub struct RepositorySecurityException(pub Exception);

impl RepositorySecurityException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self(Exception::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(
    RepositorySecurityException,
    0,
    r"Composer\Repository\RepositorySecurityException"
);
