//! ref: composer/vendor/symfony/string/Exception/InvalidArgumentException.php sha1:8925a67d1d745cb23b5bfb625c37046bf1fd4959

use crate::exception::ExceptionInterface;
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
    r"Symfony\Component\String\Exception\InvalidArgumentException"
);

impl ExceptionInterface for InvalidArgumentException {}
