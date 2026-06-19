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
            inner: Exception { message, code: 0 },
            errors,
        }
    }

    pub fn get_errors(&self) -> &Vec<String> {
        &self.errors
    }

    pub fn get_message(&self) -> &str {
        &self.inner.message
    }
}

impl std::fmt::Display for JsonValidationException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner.message)
    }
}

impl std::error::Error for JsonValidationException {}
