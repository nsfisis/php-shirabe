//! ref: composer/vendor/symfony/string/ByteString.php sha1:cb3e2b07672e990bb8bccff8b657d7cdcf697e6a

use crate::CodePointString;
use crate::exception::InvalidArgumentException;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{bformat, mb_convert_encoding, mb_detect_encoding, utf8_text};

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
    pub fn to_code_point_string(
        &self,
        from_encoding: &BStr,
    ) -> Result<CodePointString, InvalidArgumentException> {
        if matches!(
            from_encoding.as_bytes(),
            b"" | b"utf8" | b"utf-8" | b"UTF8" | b"UTF-8"
        ) && let Some(string) = utf8_text(&self.string)
        {
            return Ok(CodePointString {
                string: string.to_owned(),
            });
        }

        let from_encoding = if from_encoding.is_empty() {
            BStr::new("Windows-1252")
        } else {
            from_encoding
        };

        let valid_encoding =
            mb_detect_encoding(&self.string, Some(vec![from_encoding.to_owned()]), true).is_some();
        if !valid_encoding {
            return Err(InvalidArgumentException::new(bformat!(
                "Invalid \"{from_encoding}\" string."
            )));
        }

        let converted = mb_convert_encoding(&self.string, "UTF-8", from_encoding);
        Ok(CodePointString {
            string: utf8_text(&converted)
                .expect("mb_convert_encoding() to UTF-8 yields valid UTF-8")
                .to_owned(),
        })
    }
}
