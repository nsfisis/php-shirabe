//! ref: composer/vendor/symfony/string/ByteString.php

use crate::symfony::string::code_point_string::CodePointString;

#[derive(Debug, Clone)]
pub struct ByteString {
    pub(crate) string: String,
}

impl ByteString {
    pub fn new(_string: &str) -> Self {
        todo!()
    }

    pub fn to_code_point_string(&self, _encoding: &str) -> CodePointString {
        todo!()
    }
}
