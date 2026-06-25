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

    pub fn to_code_point_string(&self, from_encoding: &str) -> CodePointString {
        // The source `string` is always valid UTF-8 (Rust `String`/`&str` guarantee). PHP takes the
        // early-return branch whenever `preg_match('//u', ...)` holds for a UTF-8/null encoding, so
        // the result mirrors the input bytes verbatim. The `mb_detect_encoding`/`iconv` conversion
        // path only applies to genuinely non-UTF-8 byte strings, which cannot occur here.
        if matches!(from_encoding, "" | "utf8" | "utf-8" | "UTF8" | "UTF-8") {
            return CodePointString {
                string: self.string.clone(),
            };
        }

        // TODO(phase-d): non-UTF-8 source encodings would require mb_convert_encoding/iconv-style
        // decoding, which is unreachable for the UTF-8-only inputs Shirabe currently produces.
        todo!()
    }
}
