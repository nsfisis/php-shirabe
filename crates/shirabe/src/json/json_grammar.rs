//! Hand-written port of the recursive JSON grammar that `JsonManipulator` expresses in PHP via a
//! PCRE `(?(DEFINE) ... )` block with `(?&json)` subroutine calls. The `regex` crate cannot express
//! recursive subpatterns, so — per docs/dev/regex-porting.md — the grammar is decomposed here into
//! plain byte scanners. Each `scan_*` returns the byte offset just past the construct, or `None`
//! when the bytes do not match, mirroring how the PCRE grammar would fail to match un-regexable
//! content.
//!
//! The grammar being ported (from JsonManipulator::DEFINES):
//!
//! ```text
//! number   -? (?= [1-9]|0(?!\d) ) \d++ (?:\.\d++)? (?:[eE] [+-]?+ \d++)?
//! boolean  true | false | null
//! string   " (?:[^"\\]*+ | \\["\\bfnrt/] | \\u[0-9A-Fa-f]{4})* "
//! array    \[  (?: (?&json) \s*+ (?: , (?&json) \s*+ )*+ )?+  \s*+ \]
//! pair     \s*+ (?&string) \s*+ : (?&json) \s*+
//! object   \{  (?: (?&pair) (?: , (?&pair) )*+ )?+  \s*+ \}
//! json     \s*+ (?: (?&number) | (?&boolean) | (?&string) | (?&array) | (?&object) )
//! ```

/// PCRE `\s`: space, tab, newline, carriage return, form feed, vertical tab.
fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c | 0x0b)
}

pub fn skip_ws(b: &[u8], mut p: usize) -> usize {
    while p < b.len() && is_ws(b[p]) {
        p += 1;
    }
    p
}

/// `string`: a JSON string starting at `b[p] == '"'`. Returns the offset past the closing quote.
pub fn scan_string(b: &[u8], p: usize) -> Option<usize> {
    if b.get(p) != Some(&b'"') {
        return None;
    }
    let mut i = p + 1;
    loop {
        match b.get(i)? {
            b'"' => return Some(i + 1),
            b'\\' => match b.get(i + 1)? {
                b'"' | b'\\' | b'b' | b'f' | b'n' | b'r' | b't' | b'/' => i += 2,
                b'u' => {
                    for k in 0..4 {
                        if !b.get(i + 2 + k)?.is_ascii_hexdigit() {
                            return None;
                        }
                    }
                    i += 6;
                }
                _ => return None,
            },
            _ => i += 1,
        }
    }
}

/// `number`. Returns the offset past the number, or `None` (e.g. leading zeros are rejected).
pub fn scan_number(b: &[u8], p: usize) -> Option<usize> {
    let mut i = p;
    if b.get(i) == Some(&b'-') {
        i += 1;
    }
    // (?= [1-9] | 0(?!\d) )
    match b.get(i)? {
        b'1'..=b'9' => {}
        b'0' => {
            if b.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
                return None;
            }
        }
        _ => return None,
    }
    let start = i;
    while b.get(i).is_some_and(|c| c.is_ascii_digit()) {
        i += 1;
    }
    debug_assert!(i > start);
    // (?: \. \d++ )? — optional, so only consume the dot when at least one digit follows.
    if b.get(i) == Some(&b'.') && b.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
        let mut j = i + 1;
        while b.get(j).is_some_and(|c| c.is_ascii_digit()) {
            j += 1;
        }
        i = j;
    }
    // (?: [eE] [+-]?+ \d++ )? — optional; only consume when a complete exponent follows.
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(b.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        let exp = j;
        while b.get(j).is_some_and(|c| c.is_ascii_digit()) {
            j += 1;
        }
        if j > exp {
            i = j;
        }
    }
    Some(i)
}

fn scan_literal(b: &[u8], p: usize, word: &[u8]) -> Option<usize> {
    if b[p..].starts_with(word) {
        Some(p + word.len())
    } else {
        None
    }
}

/// `json`: optional leading whitespace then a single JSON value. Returns the offset past the value
/// (trailing whitespace is left for the caller, matching the grammar where `json` ends at the value).
pub fn scan_value(b: &[u8], p: usize) -> Option<usize> {
    let i = skip_ws(b, p);
    match b.get(i)? {
        b'"' => scan_string(b, i),
        b'{' => scan_object(b, i),
        b'[' => scan_array(b, i),
        b't' => scan_literal(b, i, b"true"),
        b'f' => scan_literal(b, i, b"false"),
        b'n' => scan_literal(b, i, b"null"),
        b'-' | b'0'..=b'9' => scan_number(b, i),
        _ => None,
    }
}

/// `array` starting at `b[p] == '['`. Returns the offset past the closing `]`.
pub fn scan_array(b: &[u8], p: usize) -> Option<usize> {
    let mut i = p + 1;
    let probe = skip_ws(b, i);
    if b.get(probe) == Some(&b']') {
        return Some(probe + 1);
    }
    i = scan_value(b, i)?;
    i = skip_ws(b, i);
    while b.get(i) == Some(&b',') {
        i = scan_value(b, i + 1)?;
        i = skip_ws(b, i);
    }
    if b.get(i) == Some(&b']') {
        Some(i + 1)
    } else {
        None
    }
}

/// `pair`: `\s* "key" \s* : json \s*`. Returns the offset past the trailing whitespace.
pub fn scan_pair(b: &[u8], p: usize) -> Option<usize> {
    let mut i = skip_ws(b, p);
    i = scan_string(b, i)?;
    i = skip_ws(b, i);
    if b.get(i) != Some(&b':') {
        return None;
    }
    i = scan_value(b, i + 1)?;
    Some(skip_ws(b, i))
}

/// `object` starting at `b[p] == '{'`. Returns the offset past the closing `}`.
pub fn scan_object(b: &[u8], p: usize) -> Option<usize> {
    let mut i = p + 1;
    let probe = skip_ws(b, i);
    if b.get(probe) == Some(&b'}') {
        return Some(probe + 1);
    }
    i = scan_pair(b, i)?;
    while b.get(i) == Some(&b',') {
        i = scan_pair(b, i + 1)?;
    }
    i = skip_ws(b, i);
    if b.get(i) == Some(&b'}') {
        Some(i + 1)
    } else {
        None
    }
}

/// The kind of value a top-level key must hold for a match to be accepted, mirroring whether the
/// PHP pattern captures `(?&json)`, `(?&object)` or `(?&array)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Json,
    Object,
    Array,
}

/// A located top-level key/value within a JSON object's text. All fields are byte offsets into the
/// scanned contents:
/// - `key_pos`: where the `"key"` token begins (after `\s* { \s*` and any preceding pairs)
/// - `value_pos`: where the value begins (after `"key" \s* : \s*`)
/// - `value_end`: just past the value
#[derive(Debug, Clone, Copy)]
pub struct KeyMatch {
    pub key_pos: usize,
    pub value_pos: usize,
    pub value_end: usize,
}

/// Locates a top-level key whose token text is exactly `encoded_key` (e.g. `"config"`), reproducing
/// the JsonManipulator patterns of the form
/// `\s* \{ \s* (?: (?&string) \s* : (?&json) \s* , \s* )*? KEY \s* : \s* (value)`.
///
/// Returns `None` when the object cannot be parsed up to the key, or the target value is not of the
/// requested `kind` (matching how the PCRE grammar would fail and the caller would abort).
pub fn find_top_level_key(
    contents: &[u8],
    encoded_key: &[u8],
    kind: ValueKind,
) -> Option<KeyMatch> {
    let mut i = skip_ws(contents, 0);
    if contents.get(i) != Some(&b'{') {
        return None;
    }
    i = skip_ws(contents, i + 1);
    loop {
        // Each top-level entry must start with a string key.
        if contents.get(i) != Some(&b'"') {
            return None;
        }
        let key_pos = i;
        let key_end = scan_string(contents, i)?;
        let is_target = &contents[key_pos..key_end] == encoded_key;

        let colon = skip_ws(contents, key_end);
        if contents.get(colon) != Some(&b':') {
            return None;
        }
        let value_pos = skip_ws(contents, colon + 1);

        if is_target {
            let value_end = match kind {
                ValueKind::Json => scan_value(contents, value_pos),
                ValueKind::Object if contents.get(value_pos) == Some(&b'{') => {
                    scan_object(contents, value_pos)
                }
                ValueKind::Array if contents.get(value_pos) == Some(&b'[') => {
                    scan_array(contents, value_pos)
                }
                _ => None,
            };
            // Keys are unique, so a wrong-kind target cannot match later either.
            return value_end.map(|value_end| KeyMatch {
                key_pos,
                value_pos,
                value_end,
            });
        }

        // A non-target entry must be a complete `key : value ,` pair to keep scanning.
        let value_end = scan_value(contents, value_pos)?;
        let comma = skip_ws(contents, value_end);
        if contents.get(comma) != Some(&b',') {
            return None;
        }
        i = skip_ws(contents, comma + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full(scan: fn(&[u8], usize) -> Option<usize>, s: &str) -> bool {
        scan(s.as_bytes(), 0) == Some(s.len())
    }

    #[test]
    fn strings() {
        assert!(full(scan_string, r#""hello""#));
        assert!(full(scan_string, r#""a\"b""#));
        assert!(full(scan_string, r#""é""#));
        assert!(full(scan_string, r#""""#));
        assert_eq!(scan_string(br#""abc"#, 0), None); // unterminated
        assert_eq!(scan_string(br#""\x""#, 0), None); // invalid escape
    }

    #[test]
    fn numbers() {
        assert!(full(scan_number, "0"));
        assert!(full(scan_number, "-1"));
        assert!(full(scan_number, "12.5"));
        assert!(full(scan_number, "1e10"));
        assert!(full(scan_number, "-3.14E-2"));
        assert_eq!(scan_number(b"01", 0), None); // leading zero
        assert_eq!(scan_number(b"1.", 0), Some(1)); // ".": no fraction digits -> number is just "1"
    }

    #[test]
    fn values_and_containers() {
        assert!(full(scan_value, "  true"));
        assert!(full(scan_value, "[1, 2, 3]"));
        assert!(full(scan_value, "[]"));
        assert!(full(scan_value, r#"{"a": 1, "b": [true, null]}"#));
        assert!(full(scan_value, "{}"));
        assert!(full(scan_value, "{\n    \"a\": {\n        \"b\": \"c\"\n    }\n}"));
        assert_eq!(scan_value(br#"{"a": }"#, 0), None); // missing value
        assert_eq!(scan_value(br#"{"a" 1}"#, 0), None); // missing colon
    }

    #[test]
    fn value_stops_at_end_of_value() {
        // scan_value reports the end of the value, leaving trailing content.
        assert_eq!(scan_value(br#"{"a": 1}, "rest""#, 0), Some(8));
    }

    #[test]
    fn finds_top_level_key() {
        let c = "{\n    \"foo\": \"bar\",\n    \"require\": {\n        \"a\": \"1\"\n    }\n}";
        let m = find_top_level_key(c.as_bytes(), br#""require""#, ValueKind::Object).unwrap();
        assert_eq!(&c[m.key_pos..m.value_pos], "\"require\": ");
        assert_eq!(&c[m.value_pos..m.value_end], "{\n        \"a\": \"1\"\n    }");
        // first key
        let m2 = find_top_level_key(c.as_bytes(), br#""foo""#, ValueKind::Json).unwrap();
        assert_eq!(&c[m2.value_pos..m2.value_end], "\"bar\"");
        // missing key
        assert!(find_top_level_key(c.as_bytes(), br#""nope""#, ValueKind::Json).is_none());
        // wrong kind
        assert!(find_top_level_key(c.as_bytes(), br#""foo""#, ValueKind::Array).is_none());
    }
}
