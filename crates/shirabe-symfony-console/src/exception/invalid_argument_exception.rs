//! ref: composer/vendor/symfony/console/Exception/InvalidArgumentException.php sha1:29aad04f931c6cb01ff7117c9089bcf1fbc15b35

use crate::exception::exception_interface::ExceptionInterface;
use bstr::BString;
use shirabe_php_shim::impl_php_exception;

#[derive(Debug)]
pub struct InvalidArgumentException(pub shirabe_php_shim::InvalidArgumentException);

impl InvalidArgumentException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self(shirabe_php_shim::InvalidArgumentException::new(message))
    }
}

impl_php_exception!(
    InvalidArgumentException,
    0,
    r"Symfony\Component\Console\Exception\InvalidArgumentException"
);

impl ExceptionInterface for InvalidArgumentException {}
