//! ref: composer/vendor/symfony/string/CodePointString.php sha1:38df14a7a3860f08e4aa5678665c0ec7d6765a2c

use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{mb_convert_encoding, utf8_text, wordwrap};

#[derive(Debug, Clone)]
pub struct CodePointString {
    /// PHP's CodePointString rejects anything but UTF-8 when it is built and works on code points.
    #[expect(
        clippy::disallowed_types,
        reason = "AbstractUnicodeString holds valid UTF-8 only"
    )]
    pub(crate) string: String,
}

impl CodePointString {
    /// Port of `AbstractString::wordwrap()`, specialised to the non-`ignoreCase` code-point case.
    #[expect(
        clippy::disallowed_types,
        reason = "AbstractUnicodeString splits into code points"
    )]
    pub fn wordwrap(&self, width: i64, r#break: &BStr, cut: bool) -> Self {
        // PHP throws InvalidArgumentException from split(). The delimiter is a fixed string at
        // every call site, so invalid UTF-8 here is a programming error.
        let r#break = utf8_text(r#break).expect("Split delimiter is not a valid UTF-8 string.");

        // `split($break)` with no flags reduces to `explode($break, $string)` here, then `chunk()`
        // yields one entry per code point. `ignoreCase` is always false for freshly built instances.
        let lines: Vec<&str> = if !r#break.is_empty() {
            self.string.split(r#break).collect()
        } else {
            vec![&self.string]
        };

        let mut chars: Vec<String> = Vec::new();
        let mut mask = BString::default();

        if lines.len() == 1 && lines[0].is_empty() {
            return Self {
                string: String::new(),
            };
        }

        for (i, line) in lines.iter().enumerate() {
            if i != 0 {
                chars.push(r#break.to_owned());
                mask.push(b'#');
            }

            for ch in line.chars() {
                mask.push(if ch == ' ' { b' ' } else { b'?' });
                chars.push(ch.into());
            }
        }

        let mut string = String::new();
        let mut j: usize = 0;
        // PHP seeds both `$b` and `$i` at -1; mirror with signed indices.
        let mut i: i64 = -1;
        let mask = wordwrap(&mask, width, "#", cut);
        let mask_bytes: &[u8] = mask.as_bytes();

        let mut b: i64 = -1;
        loop {
            // strpos($mask, '#', $b + 1)
            let from = (b + 1) as usize;
            let Some(rel) = mask_bytes[from..].iter().position(|&c| c == b'#') else {
                break;
            };
            b = (from + rel) as i64;

            i += 1;
            while i < b {
                string.push_str(&chars[j]);
                j += 1;
                i += 1;
            }

            if chars[j] == r#break || chars[j] == " " {
                j += 1;
            }

            string.push_str(r#break);
        }

        for c in &chars[j..] {
            string.push_str(c);
        }

        Self { string }
    }

    /// `to_encoding` is `""` for PHP's `null`.
    pub fn to_byte_string(&self, to_encoding: &BStr) -> BString {
        // A CodePointString is an AbstractUnicodeString, so PHP's `$fromEncoding` is always
        // 'UTF-8' and the string is returned verbatim for a null/UTF-8 target.
        if matches!(
            to_encoding.as_bytes(),
            b"" | b"utf8" | b"utf-8" | b"UTF8" | b"UTF-8"
        ) {
            return BString::from(self.string.as_bytes());
        }

        // PHP falls back to iconv() only when mb_convert_encoding() rejects the target encoding.
        mb_convert_encoding(&self.string, to_encoding, "UTF-8")
    }
}
