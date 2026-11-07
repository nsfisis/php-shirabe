//! ref: composer/vendor/symfony/console/Exception/LogicException.php sha1:155ed6fa39f969cbbc1728d95d00f2ebc206fdb4

use crate::exception::exception_interface::ExceptionInterface;
use bstr::BString;
use shirabe_php_shim::impl_php_exception;

#[derive(Debug)]
pub struct LogicException(pub shirabe_php_shim::LogicException);

impl LogicException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self(shirabe_php_shim::LogicException::new(message))
    }
}

impl_php_exception!(
    LogicException,
    0,
    r"Symfony\Component\Console\Exception\LogicException"
);

impl ExceptionInterface for LogicException {}
