//! ref: composer/vendor/symfony/console/Exception/RuntimeException.php sha1:a5de32b70926a03f0d8a306a01f3b75eebe60103

use crate::exception::ExceptionInterface;
use bstr::BString;
use shirabe_php_shim::impl_php_exception;

#[derive(Debug)]
pub struct RuntimeException(pub shirabe_php_shim::RuntimeException);

impl RuntimeException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self(shirabe_php_shim::RuntimeException::new(message))
    }
}

impl_php_exception!(
    RuntimeException,
    0,
    r"Symfony\Component\Console\Exception\RuntimeException"
);

impl ExceptionInterface for RuntimeException {}
