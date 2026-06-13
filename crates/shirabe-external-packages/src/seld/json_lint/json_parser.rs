//! ref: composer/vendor/seld/jsonlint/src/Seld/JsonLint/JsonParser.php

use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct JsonParser;

impl Default for JsonParser {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonParser {
    pub const DETECT_KEY_CONFLICTS: u32 = 1;

    pub fn new() -> Self {
        todo!()
    }

    pub fn parse(&self, _json: &str, _flags: u32) -> anyhow::Result<PhpMixed> {
        todo!()
    }

    /// PHP: JsonParser::lint() — returns null on success, ParsingException on failure.
    pub fn lint(&mut self, _json: &str) -> Option<crate::seld::json_lint::ParsingException> {
        todo!()
    }
}
