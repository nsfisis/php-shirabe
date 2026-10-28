//! ref: composer/vendor/symfony/string/ByteString.php

use crate::code_point_string::CodePointString;
use bstr::BString;
use shirabe_php_shim::{mb_convert_encoding, mb_detect_encoding, utf8_text};

#[derive(Debug, Clone)]
pub struct ByteString {
    string: BString,
}

impl ByteString {
    pub fn new(string: impl AsRef<[u8]>) -> Self {
        Self {
            string: BString::from(string.as_ref()),
        }
    }

    /// `from_encoding` is `""` for PHP's `null`.
    pub fn to_code_point_string(&self, from_encoding: &str) -> CodePointString {
        if matches!(from_encoding, "" | "utf8" | "utf-8" | "UTF8" | "UTF-8") {
            // PHP throws InvalidArgumentException. Callers detect `from_encoding` from this very
            // string, so invalid UTF-8 here is a programming error.
            let string = utf8_text(&self.string).expect("Invalid UTF-8 string.");
            return CodePointString {
                string: string.to_string(),
            };
        }

        let valid_encoding =
            mb_detect_encoding(&self.string, Some(vec![from_encoding.into()]), true).is_some();
        // PHP throws InvalidArgumentException. Callers detect `from_encoding` from this very
        // string, so a mismatch is a programming error.
        assert!(valid_encoding, "Invalid \"{}\" string.", from_encoding);

        let converted = mb_convert_encoding(&self.string, "UTF-8", from_encoding);
        CodePointString {
            string: utf8_text(&converted)
                .expect("mb_convert_encoding() to UTF-8 yields valid UTF-8")
                .to_string(),
        }
    }
}
