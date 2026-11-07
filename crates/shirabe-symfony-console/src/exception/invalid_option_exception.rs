//! ref: composer/vendor/symfony/console/Exception/InvalidOptionException.php sha1:51165519b00fcacc0a70597e0716e38ae3bdd159

use crate::exception::ExceptionInterface;
use bstr::BString;
use shirabe_php_shim::{InvalidArgumentException, impl_php_exception};

#[derive(Debug)]
pub struct InvalidOptionException(pub InvalidArgumentException);

impl InvalidOptionException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self(InvalidArgumentException::new(message))
    }
}

impl_php_exception!(
    InvalidOptionException,
    0,
    r"Symfony\Component\Console\Exception\InvalidOptionException"
);

impl ExceptionInterface for InvalidOptionException {}
