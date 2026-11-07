//! ref: composer/vendor/symfony/process/Exception/LogicException.php sha1:b98331d61ea70e49b2fb67fa94c63ebe5a7c61c3

use bstr::BString;
use shirabe_php_shim::impl_php_exception;

#[derive(Debug)]
pub struct LogicException {
    inner: shirabe_php_shim::LogicException,
}

impl LogicException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self {
            inner: shirabe_php_shim::LogicException::new(message),
        }
    }
}

impl_php_exception!(
    LogicException,
    inner,
    r"Symfony\Component\Process\Exception\LogicException"
);
