//! ref: composer/src/Composer/Json/JsonValidationException.php

use shirabe_php_shim::Exception;

#[derive(Debug)]
pub struct JsonValidationException {
    inner: Exception,
    pub(crate) errors: Vec<String>,
}

impl JsonValidationException {
    pub fn new(message: String, errors: Vec<String>) -> Self {
        Self {
            inner: Exception::new(message),
            errors,
        }
    }

    pub fn get_errors(&self) -> &Vec<String> {
        &self.errors
    }
}

shirabe_php_shim::impl_php_exception!(
    JsonValidationException,
    inner,
    r"Composer\Json\JsonValidationException"
);
