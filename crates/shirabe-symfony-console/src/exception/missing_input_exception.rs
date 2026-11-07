//! ref: composer/vendor/symfony/console/Exception/MissingInputException.php sha1:74cde09c0e0e708505feca111cc942abd8200149

use super::exception_interface::ExceptionInterface;
use super::runtime_exception::RuntimeException;
use bstr::BString;
use shirabe_php_shim::impl_php_exception;

#[derive(Debug)]
pub struct MissingInputException(pub RuntimeException);

impl MissingInputException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self(RuntimeException::new(message))
    }
}

impl_php_exception!(
    MissingInputException,
    0,
    r"Symfony\Component\Console\Exception\MissingInputException"
);

impl ExceptionInterface for MissingInputException {}
