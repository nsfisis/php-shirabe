//! ref: composer/vendor/symfony/string/UnicodeString.php

#[derive(Debug, Clone)]
pub struct UnicodeString {
    pub(crate) string: String,
}

impl UnicodeString {
    // TODO(phase-c): the real constructor runs `normalizer_normalize` (Unicode NFC normalization),
    // which has no Rust std equivalent and would need a dedicated normalization implementation.
    // Normalization is skipped here, which is only correct for already-NFC input such as ASCII.
    pub fn new(string: &str) -> Self {
        Self {
            string: string.to_string(),
        }
    }

    // TODO(phase-c): ASCII-only provisional implementation. The faithful `width()` uses `wcswidth`
    // with the Unicode width tables to treat wide characters as width 2 and skip zero-width /
    // combining characters; here every character counts as width 1, correct only for ASCII. The
    // ANSI/control-character stripping driven by `ignore_ansi_decoration` is likewise not handled.
    pub fn width(&self, _ignore_ansi_decoration: bool) -> i64 {
        let s = self.string.replace(['\x00', '\x05', '\x07'], "");
        let s = s.replace("\r\n", "\n").replace('\r', "\n");

        let mut width: i64 = 0;
        for line in s.split('\n') {
            let line_width = line.chars().count() as i64;
            if line_width > width {
                width = line_width;
            }
        }

        width
    }

    pub fn length(&self) -> i64 {
        todo!()
    }

    pub fn slice(&self, _start: i64, _length: Option<i64>) -> Self {
        todo!()
    }
}
