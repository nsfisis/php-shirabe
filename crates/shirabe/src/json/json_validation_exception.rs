//! ref: composer/src/Composer/Json/JsonValidationException.php

use bstr::BString;
use shirabe_php_shim::Exception;

#[derive(Debug)]
pub struct JsonValidationException {
    inner: Exception,
    errors: Vec<BString>,
}

impl JsonValidationException {
    #[track_caller]
    pub fn new(message: impl Into<BString>, errors: Vec<BString>) -> Self {
        Self {
            inner: Exception::new(message),
            errors,
        }
    }

    pub fn get_errors(&self) -> &Vec<BString> {
        &self.errors
    }
}

shirabe_php_shim::impl_php_exception!(
    JsonValidationException,
    inner,
    r"Composer\Json\JsonValidationException"
);
