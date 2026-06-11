use super::exception_interface::ExceptionInterface;
use super::invalid_argument_exception::InvalidArgumentException;

#[derive(Debug)]
pub struct InvalidOptionException(pub InvalidArgumentException);

impl std::fmt::Display for InvalidOptionException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for InvalidOptionException {}

impl ExceptionInterface for InvalidOptionException {}
