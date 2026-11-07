//! ref: composer/src/Composer/Json/JsonValidationException.php sha1:12472643e8abf0e6f472b8f333040bf2d4501f24

use bstr::BString;
use shirabe_php_shim::{Exception, impl_php_exception};

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

impl_php_exception!(
    JsonValidationException,
    inner,
    r"Composer\Json\JsonValidationException"
);
