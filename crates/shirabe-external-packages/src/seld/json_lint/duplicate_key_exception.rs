//! ref: composer/vendor/seld/jsonlint/src/Seld/JsonLint/DuplicateKeyException.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct DuplicateKeyException {
    pub message: String,
    pub code: i64,
    pub details: IndexMap<String, PhpMixed>,
}

impl DuplicateKeyException {
    pub fn get_details(&self) -> &IndexMap<String, PhpMixed> {
        &self.details
    }
}

impl std::fmt::Display for DuplicateKeyException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for DuplicateKeyException {}
