//! ref: composer/vendor/composer/pcre/src/UnexpectedNullMatchException.php

use super::pcre_exception::PcreException;

#[derive(Debug)]
pub struct UnexpectedNullMatchException(pub PcreException);

impl UnexpectedNullMatchException {
    pub fn new(message: String) -> UnexpectedNullMatchException {
        UnexpectedNullMatchException(PcreException(shirabe_php_shim::RuntimeException {
            message,
            code: 0,
        }))
    }

    pub fn from_function(_function: &str, _pattern: &str) -> UnexpectedNullMatchException {
        panic!(
            "fromFunction should not be called on UnexpectedNullMatchException, use PcreException"
        );
    }
}

impl std::fmt::Display for UnexpectedNullMatchException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for UnexpectedNullMatchException {}
