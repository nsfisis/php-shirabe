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
        let mask = php_wordwrap(&mask, width, "#", cut);
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

    pub fn to_byte_string(&self, to_encoding: &str) -> String {
        // The source is always valid UTF-8, so PHP's `toByteString` returns the string verbatim
        // whenever the target is null/UTF-8 (the only encodings reached here). The
        // mb_convert_encoding/iconv path applies only to non-UTF-8 targets, which do not occur.
        if matches!(to_encoding, "" | "utf8" | "utf-8" | "UTF8" | "UTF-8") {
            return self.string.clone();
        }

        // TODO(phase-d): converting to a non-UTF-8 target encoding needs mb_convert_encoding/iconv,
        // unreachable for Shirabe's UTF-8-only output.
        todo!()
    }
}

/// Port of PHP's built-in `wordwrap()` (`PHP_FUNCTION(wordwrap)` in ext/standard/string.c).
/// Byte-based, matching PHP's single-byte/multi-byte break and cut handling.
fn php_wordwrap(text: &str, linelength: i64, breakchar: &str, docut: bool) -> String {
    let text = text.as_bytes();
    let breakchar = breakchar.as_bytes();
    let textlen = text.len() as i64;
    let breaklen = breakchar.len() as i64;

    if textlen == 0 {
        return String::new();
    }

    let mut laststart: i64 = 0;
    let mut lastspace: i64 = 0;

    // Special case for a single-character break that needs no extra storage.
    if breaklen == 1 && !docut {
        let mut out = text.to_vec();
        let mut current = 0i64;
        while current < textlen {
            let c = out[current as usize];
            if c == breakchar[0] {
                laststart = current + 1;
                lastspace = current + 1;
            } else if c == b' ' {
                if current - laststart >= linelength {
                    out[current as usize] = breakchar[0];
                    laststart = current + 1;
                }
                lastspace = current;
            } else if current - laststart >= linelength && laststart != lastspace {
                out[lastspace as usize] = breakchar[0];
                laststart = lastspace + 1;
            }
            current += 1;
        }
        return String::from_utf8_lossy(&out).into_owned();
    }

    // Multiple character line break or forced cut.
    let mut out: Vec<u8> = Vec::new();
    let mut current = 0i64;
    while current < textlen {
        // When we hit an existing break, copy to the new buffer and fix up laststart/lastspace.
        if text[current as usize] == breakchar[0]
            && current + breaklen < textlen
            && &text[current as usize..(current + breaklen) as usize] == breakchar
        {
            out.extend_from_slice(&text[laststart as usize..(current + breaklen) as usize]);
            current += breaklen - 1;
            laststart = current + 1;
            lastspace = current + 1;
        } else if text[current as usize] == b' ' {
            if current - laststart >= linelength {
                out.extend_from_slice(&text[laststart as usize..current as usize]);
                out.extend_from_slice(breakchar);
                laststart = current + 1;
            }
            lastspace = current;
        } else if current - laststart >= linelength && docut && laststart >= lastspace {
            out.extend_from_slice(&text[laststart as usize..current as usize]);
            out.extend_from_slice(breakchar);
            laststart = current;
            lastspace = current;
        } else if current - laststart >= linelength && laststart < lastspace {
            out.extend_from_slice(&text[laststart as usize..lastspace as usize]);
            out.extend_from_slice(breakchar);
            laststart = lastspace + 1;
            lastspace += 1;
        }
        current += 1;
    }

    // Copy over any stragglers.
    if laststart != current {
        out.extend_from_slice(&text[laststart as usize..current as usize]);
    }

    String::from_utf8_lossy(&out).into_owned()
}
