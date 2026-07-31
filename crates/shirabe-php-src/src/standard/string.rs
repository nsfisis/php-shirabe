/// php-src: ext/standard/string.c `php_charmask` (PHP 8.5.2)
///
/// Build the set of bytes to strip from a PHP trim `$characters` argument, expanding `a..b`
/// range syntax as PHP does. The original's four warning branches for malformed `..` ranges are
/// not ported.
pub fn php_trim_mask(chars: &[u8]) -> [bool; 256] {
    let mut mask = [false; 256];
    let mut i = 0;
    while i < chars.len() {
        if i + 3 < chars.len() && chars[i + 1] == b'.' && chars[i + 2] == b'.' {
            let start = chars[i];
            let end = chars[i + 3];
            if start <= end {
                for b in start..=end {
                    mask[b as usize] = true;
                }
                i += 4;
                continue;
            }
        }
        mask[chars[i] as usize] = true;
        i += 1;
    }
    mask
}

/// php-src: ext/standard/string.c `php_addcslashes_str` (PHP 8.5.2)
///
/// Every byte that falls in the (range-expanded) charlist is backslash-escaped, with
/// non-printable bytes rendered as the C escape or a three-digit octal.
pub fn addcslashes(_string: &str, _charlist: &str) -> String {
    let mask = php_trim_mask(_charlist.as_bytes());
    let mut out: Vec<u8> = Vec::with_capacity(_string.len());
    for &c in _string.as_bytes() {
        if mask[c as usize] {
            if !(32..=126).contains(&c) {
                out.push(b'\\');
                match c {
                    b'\n' => out.push(b'n'),
                    b'\t' => out.push(b't'),
                    b'\r' => out.push(b'r'),
                    0x07 => out.push(b'a'),
                    0x0B => out.push(b'v'),
                    0x08 => out.push(b'b'),
                    0x0C => out.push(b'f'),
                    _ => out.extend_from_slice(format!("{:03o}", c).as_bytes()),
                }
            } else {
                out.push(b'\\');
                out.push(c);
            }
        } else {
            out.push(c);
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// php-src: ext/standard/string.c `php_stripcslashes` (PHP 8.5.2)
///
/// The inverse of `addcslashes`, decoding C escape sequences including octal (\ooo) and hex
/// (\xHH).
pub fn stripcslashes(_s: &str) -> String {
    let bytes = _s.as_bytes();
    let n = bytes.len();
    let mut out: Vec<u8> = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        if bytes[i] == b'\\' && i + 1 < n {
            i += 1;
            match bytes[i] {
                b'n' => {
                    out.push(b'\n');
                    i += 1;
                }
                b'r' => {
                    out.push(b'\r');
                    i += 1;
                }
                b'a' => {
                    out.push(0x07);
                    i += 1;
                }
                b't' => {
                    out.push(b'\t');
                    i += 1;
                }
                b'v' => {
                    out.push(0x0B);
                    i += 1;
                }
                b'b' => {
                    out.push(0x08);
                    i += 1;
                }
                b'f' => {
                    out.push(0x0C);
                    i += 1;
                }
                b'\\' => {
                    out.push(b'\\');
                    i += 1;
                }
                b'x' => {
                    if i + 1 < n && bytes[i + 1].is_ascii_hexdigit() {
                        let mut val: u8 = 0;
                        let mut count = 0;
                        i += 1;
                        while i < n && count < 2 && bytes[i].is_ascii_hexdigit() {
                            val = val.wrapping_mul(16) + hex_digit_value(bytes[i]).unwrap();
                            i += 1;
                            count += 1;
                        }
                        out.push(val);
                    } else {
                        out.push(b'x');
                        i += 1;
                    }
                }
                b'0'..=b'7' => {
                    let mut val: u8 = 0;
                    let mut count = 0;
                    while i < n && count < 3 && (b'0'..=b'7').contains(&bytes[i]) {
                        val = val.wrapping_mul(8).wrapping_add(bytes[i] - b'0');
                        i += 1;
                        count += 1;
                    }
                    out.push(val);
                }
                other => {
                    out.push(other);
                    i += 1;
                }
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_digit_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// php-src: ext/standard/string.c `php_strip_tags_ex` (PHP 8.5.2)
///
/// The allowed-tags parameter is omitted from this signature.
/// State: 0 = text, 1 = inside a tag, 2 = inside an HTML comment, 3 = inside `<? ... ?>` / `<!`.
/// TODO(phase-d): this omits allowed-tags handling and the tag-depth counter, so it can diverge
/// from PHP on malformed markup (unterminated comments/quotes, nested `<`).
pub fn strip_tags(_str: &str) -> String {
    let bytes = _str.as_bytes();
    let n = bytes.len();
    let mut out: Vec<u8> = Vec::with_capacity(n);
    let mut state: u8 = 0;
    // Quote char while inside a quoted attribute value, or 0.
    let mut in_q: u8 = 0;
    let mut i = 0;
    while i < n {
        let c = bytes[i];
        match c {
            b'<' => {
                if in_q == 0 {
                    if state == 0 && i + 1 < n && bytes[i + 1].is_ascii_whitespace() {
                        // PHP keeps "< " (a `<` followed by whitespace) as literal text.
                        out.push(c);
                    } else if state == 0 {
                        state = 1;
                    }
                }
            }
            b'>' => {
                if in_q == 0 {
                    match state {
                        1 | 3 => state = 0,
                        2 => {
                            if i >= 2 && bytes[i - 1] == b'-' && bytes[i - 2] == b'-' {
                                state = 0;
                            }
                        }
                        _ => out.push(c),
                    }
                }
            }
            b'"' | b'\'' => {
                if state == 1 {
                    if in_q == 0 {
                        in_q = c;
                    } else if in_q == c && !(i > 0 && bytes[i - 1] == b'\\') {
                        in_q = 0;
                    }
                } else if state == 0 {
                    out.push(c);
                }
            }
            b'!' => {
                if state == 1 && i > 0 && bytes[i - 1] == b'<' {
                    state = 3;
                } else if state == 0 {
                    out.push(c);
                }
            }
            b'?' => {
                if state == 1 && i > 0 && bytes[i - 1] == b'<' {
                    state = 3;
                } else if state == 0 {
                    out.push(c);
                }
            }
            b'-' => {
                if state == 3 && i >= 2 && bytes[i - 1] == b'-' && bytes[i - 2] == b'!' {
                    state = 2;
                } else if state == 0 {
                    out.push(c);
                }
            }
            _ => {
                if state == 0 {
                    out.push(c);
                }
            }
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// php-src: ext/standard/string.c `PHP_FUNCTION(wordwrap)` (PHP 8.5.2)
///
/// Byte-based, matching PHP's single-byte/multi-byte break and cut handling. The original's
/// output buffer pre-allocation and growth (`chk` / `alloced` / `newtextlen`) is replaced by
/// pushing onto a `Vec`. Argument validation stays with the caller.
pub fn php_wordwrap(text: &str, linelength: i64, breakchar: &str, docut: bool) -> String {
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
