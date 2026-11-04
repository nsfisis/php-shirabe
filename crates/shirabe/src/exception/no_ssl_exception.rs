//! ref: composer/src/Composer/Exception/NoSslException.php sha1:028672aabe912923f0bb37f7bb2ef7bd6df2a31b

use bstr::BString;
use shirabe_php_shim::RuntimeException;

/// Specific exception for Composer\Util\HttpDownloader creation.
#[derive(Debug)]
pub struct NoSslException(pub RuntimeException);

impl NoSslException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self(RuntimeException::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(NoSslException, 0, r"Composer\Exception\NoSslException");
