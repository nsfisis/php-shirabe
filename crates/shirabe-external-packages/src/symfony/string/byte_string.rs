//! ref: composer/vendor/symfony/string/ByteString.php

use crate::symfony::string::code_point_string::CodePointString;

#[derive(Debug, Clone)]
pub struct ByteString {
    pub(crate) string: String,
}

impl ByteString {
    pub fn new(string: &str) -> Self {
        Self {
            string: string.to_string(),
        }
    }

    /// `from_encoding` is `""` for PHP's `null`.
    pub fn to_code_point_string(&self, from_encoding: &str) -> CodePointString {
        // The source `string` is always valid UTF-8 (Rust `String`/`&str` guarantee), so
        // `preg_match('//u', ...)` always holds.
        if matches!(from_encoding, "" | "utf8" | "utf-8" | "UTF8" | "UTF-8") {
            return CodePointString {
                string: self.string.clone(),
            };
        }

        let valid_encoding = shirabe_php_shim::mb_detect_encoding(
            &self.string,
            Some(vec![from_encoding.to_string()]),
            true,
        )
        .is_some();
        // PHP throws InvalidArgumentException. Callers detect `from_encoding` from this very
        // string, so a mismatch is a programming error.
        assert!(valid_encoding, "Invalid \"{}\" string.", from_encoding);

        CodePointString {
            string: shirabe_php_shim::mb_convert_encoding(
                self.string.clone().into_bytes(),
                "UTF-8",
                from_encoding,
            ),
        }
    }
}
