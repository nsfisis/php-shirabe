//! ref: composer/vendor/symfony/string/CodePointString.php

#[derive(Debug, Clone)]
pub struct CodePointString {
    pub(crate) string: String,
}

impl CodePointString {
    /// Port of `AbstractString::wordwrap()`, specialised to the non-`ignoreCase` code-point case.
    pub fn wordwrap(&self, width: i64, r#break: &str, cut: bool) -> Self {
        // `split($break)` with no flags reduces to `explode($break, $string)` here, then `chunk()`
        // yields one entry per code point. `ignoreCase` is always false for freshly built instances.
        let lines: Vec<&str> = if !r#break.is_empty() {
            self.string.split(r#break).collect()
        } else {
            vec![&self.string]
        };

        let mut chars: Vec<String> = Vec::new();
        let mut mask = String::new();

        if lines.len() == 1 && lines[0].is_empty() {
            return Self {
                string: String::new(),
            };
        }

        for (i, line) in lines.iter().enumerate() {
            if i != 0 {
                chars.push(r#break.to_string());
                mask.push('#');
            }

            for ch in line.chars() {
                let s = ch.to_string();
                mask.push(if s == " " { ' ' } else { '?' });
                chars.push(s);
            }
        }

        let mut string = String::new();
        let mut j: usize = 0;
        // PHP seeds both `$b` and `$i` at -1; mirror with signed indices.
        let mut i: i64 = -1;
        let mask = shirabe_php_shim::wordwrap(&mask, width, "#", cut);
        let mask_bytes = mask.as_bytes();

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
    pub fn to_byte_string(&self, to_encoding: &str) -> String {
        // A CodePointString is an AbstractUnicodeString, so PHP's `$fromEncoding` is always
        // 'UTF-8' and the string is returned verbatim for a null/UTF-8 target.
        if matches!(to_encoding, "" | "utf8" | "utf-8" | "UTF8" | "UTF-8") {
            return self.string.clone();
        }

        // PHP falls back to iconv() only when mb_convert_encoding() rejects the target encoding.
        shirabe_php_shim::mb_convert_encoding(
            self.string.clone().into_bytes(),
            to_encoding,
            "UTF-8",
        )
    }
}
