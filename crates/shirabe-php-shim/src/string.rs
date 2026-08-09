use crate::PhpMixed;
use indexmap::IndexMap;
pub use shirabe_php_src::standard::string::{addcslashes, strip_tags, stripcslashes};
use shirabe_php_src::standard::string::{php_trim_mask, php_wordwrap};
use shirabe_php_src::standard::strnatcmp::strnatcmp_ex;

pub fn str_replace(search: &str, replace: &str, subject: &str) -> String {
    // PHP returns the subject unchanged when the search string is empty, whereas Rust's
    // `str::replace` would insert `replace` between every character.
    if search.is_empty() {
        return subject.to_string();
    }

    subject.replace(search, replace)
}

pub fn str_contains(haystack: &str, needle: &str) -> bool {
    haystack.contains(needle)
}

pub fn str_starts_with(haystack: &str, needle: &str) -> bool {
    haystack.starts_with(needle)
}

pub fn str_ends_with(haystack: &str, needle: &str) -> bool {
    haystack.ends_with(needle)
}

pub fn substr_count(haystack: &str, needle: &str) -> i64 {
    if needle.is_empty() {
        panic!("substr_count(): Argument #2 ($needle) cannot be empty");
    }
    // str::matches counts non-overlapping occurrences, matching PHP's substr_count.
    haystack.matches(needle).count() as i64
}

// Byte-based, matching PHP's substr_replace.
// TODO(phase-c): PHP accepts negative $start/$length (counting from the end); this signature takes
// usize and therefore cannot express those cases.
pub fn substr_replace(string: &str, replace: &str, start: usize, length: usize) -> String {
    let bytes = string.as_bytes();
    let start = start.min(bytes.len());
    let end = start.saturating_add(length).min(bytes.len());
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len() + replace.len());
    out.extend_from_slice(&bytes[..start]);
    out.extend_from_slice(replace.as_bytes());
    out.extend_from_slice(&bytes[end..]);
    String::from_utf8_lossy(&out).into_owned()
}

pub fn str_repeat(s: &str, count: usize) -> String {
    s.repeat(count)
}

pub fn str_replace_array(search: &[String], replace: &[String], subject: &str) -> String {
    // PHP's array form of str_replace replaces each search element in order with the replace
    // element at the same index, falling back to an empty string when replace is shorter.
    let mut result = subject.to_string();
    for (i, s) in search.iter().enumerate() {
        let r = replace.get(i).map(String::as_str).unwrap_or("");
        result = str_replace(s, r, &result);
    }
    result
}

pub fn str_pad(input: &str, length: usize, pad_string: &str, pad_type: i64) -> String {
    // PHP str_pad() works on bytes: it pads up to `length` bytes by repeating `pad_string`.
    let input_len = input.len();
    if length <= input_len || pad_string.is_empty() {
        return input.to_string();
    }
    let pad = pad_string.as_bytes();
    let make = |n: usize| -> Vec<u8> { (0..n).map(|i| pad[i % pad.len()]).collect() };
    let total = length - input_len;
    let mut out: Vec<u8> = Vec::with_capacity(length);
    match pad_type {
        STR_PAD_LEFT => {
            out.extend(make(total));
            out.extend_from_slice(input.as_bytes());
        }
        STR_PAD_BOTH => {
            let left = total / 2;
            out.extend(make(left));
            out.extend_from_slice(input.as_bytes());
            out.extend(make(total - left));
        }
        _ => {
            out.extend_from_slice(input.as_bytes());
            out.extend(make(total));
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub const STR_PAD_LEFT: i64 = 0;
pub const STR_PAD_RIGHT: i64 = 1;
pub const STR_PAD_BOTH: i64 = 2;

pub fn str_split(s: &str, length: i64) -> Vec<String> {
    // PHP str_split() chunks the string by bytes into pieces of `length` bytes.
    let length = length.max(1) as usize;
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return vec![String::new()];
    }
    bytes
        .chunks(length)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect()
}

pub fn str_bitand(a: &str, b: &str) -> String {
    // PHP's string `&` operator: byte-wise AND, the result truncated to the shorter operand.
    let a = a.as_bytes();
    let b = b.as_bytes();
    let n = a.len().min(b.len());
    let out: Vec<u8> = (0..n).map(|i| a[i] & b[i]).collect();
    String::from_utf8_lossy(&out).into_owned()
}

pub fn str_replace_arrays(search: &[String], replace: &[String], subject: &str) -> String {
    str_replace_array(search, replace, subject)
}

pub fn str_replace_arr(search: &[&str], replace: &str, subject: &str) -> String {
    // PHP str_replace(array, string, subject): every search element is replaced with
    // the same replacement string, applied in order.
    let mut result = subject.to_string();
    for s in search {
        result = str_replace(s, replace, &result);
    }
    result
}

pub fn strcasecmp(s1: &str, s2: &str) -> i64 {
    s1.to_ascii_lowercase().cmp(&s2.to_ascii_lowercase()) as i64
}

pub fn strpos(haystack: &str, needle: &str) -> Option<usize> {
    haystack.find(needle)
}

pub fn strtoupper(s: &str) -> String {
    s.to_ascii_uppercase()
}

pub fn strlen(s: &str) -> i64 {
    s.len() as i64
}

pub fn strtr(str: &str, from: &str, to: &str) -> String {
    let from: Vec<char> = from.chars().collect();
    let to: Vec<char> = to.chars().collect();
    let n = from.len().min(to.len());
    str.chars()
        .map(|c| match from[..n].iter().position(|&f| f == c) {
            Some(i) => to[i],
            None => c,
        })
        .collect()
}

pub fn strpbrk(haystack: &str, char_list: &str) -> Option<String> {
    let set = char_list.as_bytes();
    let bytes = haystack.as_bytes();
    for i in 0..bytes.len() {
        if set.contains(&bytes[i]) {
            return Some(String::from_utf8_lossy(&bytes[i..]).into_owned());
        }
    }
    None
}

pub fn strnatcasecmp(s1: &str, s2: &str) -> i64 {
    strnatcmp_ex(s1.as_bytes(), s2.as_bytes(), true)
}

pub fn strrpos(haystack: &str, needle: &str) -> Option<usize> {
    haystack.rfind(needle)
}

// Byte-based, matching PHP: strrev() reverses the bytes, not the characters.
pub fn strrev(s: &str) -> String {
    let mut bytes = s.as_bytes().to_vec();
    bytes.reverse();
    String::from_utf8_lossy(&bytes).into_owned()
}

pub fn strtolower(s: &str) -> String {
    s.to_ascii_lowercase()
}

pub fn stripos(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .to_ascii_lowercase()
        .find(needle.to_ascii_lowercase().as_str())
}

// Byte-based, matching PHP's array form of strtr: at each position the longest
// matching key wins (insertion order breaks ties), and replacements are not
// re-scanned. Empty keys are ignored.
pub fn strtr_array(s: &str, pairs: &IndexMap<String, String>) -> String {
    let mut keys: Vec<&String> = pairs.keys().filter(|k| !k.is_empty()).collect();
    keys.sort_by_key(|k| std::cmp::Reverse(k.len()));

    let bytes = s.as_bytes();
    let mut result: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let mut matched = false;
        for key in &keys {
            let kb = key.as_bytes();
            if bytes[i..].starts_with(kb) {
                result.extend_from_slice(pairs[*key].as_bytes());
                i += kb.len();
                matched = true;
                break;
            }
        }
        if !matched {
            result.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&result).into_owned()
}

pub fn strcmp(s1: &str, s2: &str) -> i64 {
    s1.cmp(s2) as i64
}

pub fn strnatcmp(s1: &str, s2: &str) -> i64 {
    strnatcmp_ex(s1.as_bytes(), s2.as_bytes(), false)
}

pub fn strcspn(string: &str, characters: &str) -> usize {
    let set = characters.as_bytes();
    let mut count = 0;
    for &b in string.as_bytes() {
        if set.contains(&b) {
            break;
        }
        count += 1;
    }
    count
}

pub fn strstr(haystack: &str, needle: &str) -> Option<String> {
    haystack.find(needle).map(|i| haystack[i..].to_string())
}

pub fn strstr3(haystack: &str, needle: &str, before_needle: bool) -> Option<String> {
    haystack.find(needle).map(|i| {
        if before_needle {
            haystack[..i].to_string()
        } else {
            haystack[i..].to_string()
        }
    })
}

/// PHP's default trim character mask: " \t\n\r\0\x0B".
const PHP_TRIM_DEFAULT_CHARS: &[u8] = b" \t\n\r\0\x0B";

pub fn rtrim(s: &str, chars: Option<&str>) -> String {
    let mask = php_trim_mask(
        chars
            .map(|c| c.as_bytes())
            .unwrap_or(PHP_TRIM_DEFAULT_CHARS),
    );
    let bytes = s.as_bytes();
    let mut end = bytes.len();
    while end > 0 && mask[bytes[end - 1] as usize] {
        end -= 1;
    }
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

pub fn ltrim(s: &str, chars: Option<&str>) -> String {
    let mask: Vec<char> = match chars {
        Some(c) => c.chars().collect(),
        None => vec![' ', '\t', '\n', '\r', '\0', '\x0B'],
    };
    s.trim_start_matches(|c| mask.contains(&c)).to_string()
}

pub fn trim(s: &str, chars: Option<&str>) -> String {
    let mask: Vec<char> = match chars {
        Some(c) => c.chars().collect(),
        None => vec![' ', '\t', '\n', '\r', '\0', '\x0B'],
    };
    s.trim_matches(|c| mask.contains(&c)).to_string()
}

// Byte-based, matching PHP's substr. A negative start/length counts from the end.
// The result is reinterpreted as UTF-8 (lossily), which only matters when a slice
// boundary falls inside a multibyte sequence.
pub fn substr(s: &str, start: i64, length: Option<i64>) -> String {
    let bytes = s.as_bytes();
    let len = bytes.len() as i64;
    let start = if start < 0 {
        (len + start).max(0)
    } else {
        start.min(len)
    };
    let end = match length {
        None => len,
        Some(l) if l < 0 => (len + l).max(start),
        Some(l) => (start + l).min(len),
    };
    String::from_utf8_lossy(&bytes[start as usize..end as usize]).into_owned()
}

pub fn implode(glue: &str, pieces: &[String]) -> String {
    pieces.join(glue)
}

pub fn explode(delimiter: &str, string: &str) -> Vec<String> {
    string.split(delimiter).map(|s| s.to_string()).collect()
}

fn explode_limit_impl(delimiter: &str, string: &str, limit: i64) -> Vec<String> {
    if limit > 0 {
        string
            .splitn(limit as usize, delimiter)
            .map(|s| s.to_string())
            .collect()
    } else if limit == 0 {
        // PHP treats a zero limit as 1: the whole string is returned as one element.
        vec![string.to_string()]
    } else {
        let parts: Vec<String> = string.split(delimiter).map(|s| s.to_string()).collect();
        let keep = parts.len() as i64 + limit;
        if keep <= 0 {
            Vec::new()
        } else {
            parts[..keep as usize].to_vec()
        }
    }
}

pub fn explode_with_limit(delimiter: &str, string: &str, limit: i64) -> Vec<String> {
    explode_limit_impl(delimiter, string, limit)
}

pub fn explode_limit(delimiter: &str, string: &str, limit: i64) -> Vec<String> {
    explode_limit_impl(delimiter, string, limit)
}

/// Normalizes an mbstring encoding label to a canonical spelling (e.g. `utf8` -> `UTF-8`).
fn canonical_encoding(name: &str) -> String {
    match name.to_ascii_uppercase().replace('-', "").as_str() {
        "UTF8" => "UTF-8".to_string(),
        "ASCII" | "USASCII" => "ASCII".to_string(),
        _ => name.to_ascii_uppercase(),
    }
}

pub fn mb_convert_encoding(string: Vec<u8>, to_encoding: &str, from_encoding: &str) -> String {
    let to = canonical_encoding(to_encoding);
    let from = canonical_encoding(from_encoding);
    // ASCII is a subset of UTF-8, so converting among ASCII/UTF-8 is a byte-level no-op. Other
    // encodings need conversion tables that have not been ported yet.
    if matches!(to.as_str(), "UTF-8" | "ASCII") && matches!(from.as_str(), "UTF-8" | "ASCII") {
        return String::from_utf8_lossy(&string).into_owned();
    }
    todo!("mb_convert_encoding {} -> {}", from, to)
}

pub fn mb_strlen(s: &str, _encoding: &str) -> i64 {
    // `s` is valid UTF-8, so the character count is its number of code points.
    s.chars().count() as i64
}

pub fn mb_check_encoding(value: &str, encoding: &str) -> bool {
    match encoding.to_ascii_uppercase().replace('-', "").as_str() {
        // A Rust &str is, by construction, valid UTF-8.
        "UTF8" => true,
        "ASCII" | "USASCII" => value.is_ascii(),
        // Other encodings need the mbstring validation tables, which have not been ported.
        _ => todo!(),
    }
}

pub fn mb_detect_encoding(
    s: &str,
    encodings: Option<Vec<String>>,
    _strict: bool,
) -> Option<String> {
    // PHP's default detection order is ASCII then UTF-8. `s` is already valid UTF-8, so detection
    // reduces to: pure-ASCII content matches "ASCII", anything else matches "UTF-8".
    let order = encodings.unwrap_or_else(|| vec!["ASCII".to_string(), "UTF-8".to_string()]);
    for enc in order {
        match canonical_encoding(&enc).as_str() {
            "ASCII" if s.is_ascii() => return Some(enc),
            "UTF-8" => return Some(enc),
            _ => {}
        }
    }
    None
}

pub fn mb_strwidth(s: &str, _encoding: Option<&str>) -> i64 {
    // TODO(phase-c): calculate actual width
    s.len() as i64
}

pub fn mb_substr(s: &str, start: i64, length: Option<i64>, _encoding: Option<&str>) -> String {
    // Code-point based, mirroring substr's byte-based offset/length handling.
    let chars: Vec<char> = s.chars().collect();
    let (start, end) = php_slice_bounds(chars.len() as i64, start, length);
    chars[start..end].iter().collect()
}

pub fn mb_str_split(s: &str, length: i64) -> Vec<String> {
    let length = length.max(1) as usize;
    let chars: Vec<char> = s.chars().collect();
    chars
        .chunks(length)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

pub fn mb_convert_variables(to: &str, from: &str, vars: &mut [String]) -> Option<String> {
    // Converts each variable in place from `from` to `to`, returning the source encoding (PHP
    // returns the detected source encoding; here `from` is a single named encoding).
    for v in vars.iter_mut() {
        *v = mb_convert_encoding(std::mem::take(v).into_bytes(), to, from);
    }
    Some(from.to_string())
}

/// Resolve PHP array_slice/substr-style (offset, length) into a `[start, end)`
/// pair of indices, honouring negative offsets and lengths.
fn php_slice_bounds(len: i64, offset: i64, length: Option<i64>) -> (usize, usize) {
    let start = if offset < 0 {
        (len + offset).max(0)
    } else {
        offset.min(len)
    };
    let end = match length {
        None => len,
        Some(l) if l < 0 => (len + l).max(start),
        Some(l) => (start + l).min(len),
    };
    (start as usize, end as usize)
}

pub fn rawurldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) =
                (hex_digit_value(bytes[i + 1]), hex_digit_value(bytes[i + 2]))
        {
            out.push((h << 4) | l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn rawurlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

pub fn base64_encode(data: impl AsRef<[u8]>) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = data.as_ref();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b1 = chunk.get(1).copied();
        let b2 = chunk.get(2).copied();
        let n = (chunk[0] as u32) << 16 | (b1.unwrap_or(0) as u32) << 8 | (b2.unwrap_or(0) as u32);
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if b1.is_some() {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if b2.is_some() {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

pub fn base64_decode(data: &str) -> Option<Vec<u8>> {
    // Non-strict mode (PHP's default $strict = false): characters outside the base64 alphabet are
    // silently skipped, and padding terminates the input.
    let mut sextets: Vec<u8> = Vec::with_capacity(data.len());
    for &b in data.as_bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => continue,
        };
        sextets.push(v);
    }
    let mut out = Vec::with_capacity(sextets.len() * 3 / 4);
    for chunk in sextets.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let n = (chunk[0] as u32) << 18
            | (chunk[1] as u32) << 12
            | (chunk.get(2).copied().unwrap_or(0) as u32) << 6
            | (chunk.get(3).copied().unwrap_or(0) as u32);
        out.push((n >> 16) as u8);
        if chunk.len() >= 3 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() >= 4 {
            out.push(n as u8);
        }
    }
    Some(out)
}

pub fn ctype_alnum(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

pub fn ctype_digit(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

pub fn ord(c: &str) -> i64 {
    c.as_bytes().first().copied().unwrap_or(0) as i64
}

pub fn ucwords(s: &str) -> String {
    // PHP's default word delimiters: space, tab, CR, LF, FF and VT.
    let delimiters = [' ', '\t', '\r', '\n', '\x0C', '\x0B'];
    let mut out = String::with_capacity(s.len());
    let mut capitalize_next = true;
    for c in s.chars() {
        if capitalize_next {
            out.push(c.to_ascii_uppercase());
        } else {
            out.push(c);
        }
        capitalize_next = delimiters.contains(&c);
    }
    out
}

fn hex_digit_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub fn sprintf(format: &str, args: &[PhpMixed]) -> String {
    let fb = format.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    let mut next_arg = 0usize;
    while i < fb.len() {
        if fb[i] != b'%' {
            // Copy the literal run verbatim, preserving any multibyte sequences.
            let start = i;
            while i < fb.len() && fb[i] != b'%' {
                i += 1;
            }
            out.push_str(&format[start..i]);
            continue;
        }
        i += 1;
        if i >= fb.len() {
            out.push('%');
            break;
        }
        if fb[i] == b'%' {
            out.push('%');
            i += 1;
            continue;
        }

        // Optional positional argument: "n$".
        let mut explicit_arg: Option<usize> = None;
        {
            let mut k = i;
            while k < fb.len() && fb[k].is_ascii_digit() {
                k += 1;
            }
            if k > i && k < fb.len() && fb[k] == b'$' {
                explicit_arg = format[i..k].parse::<usize>().ok();
                i = k + 1;
            }
        }

        // Flags.
        let mut left = false;
        let mut plus = false;
        let mut space = false;
        let mut pad: u8 = b' ';
        loop {
            if i >= fb.len() {
                break;
            }
            match fb[i] {
                b'-' => left = true,
                b'+' => plus = true,
                b' ' => space = true,
                b'0' => pad = b'0',
                b'\'' => {
                    i += 1;
                    if i < fb.len() {
                        pad = fb[i];
                    }
                }
                _ => break,
            }
            i += 1;
        }

        // Width.
        let mut width = 0usize;
        {
            let start = i;
            while i < fb.len() && fb[i].is_ascii_digit() {
                i += 1;
            }
            if i > start {
                width = format[start..i].parse().unwrap_or(0);
            }
        }

        // Precision.
        let mut precision: Option<usize> = None;
        if i < fb.len() && fb[i] == b'.' {
            i += 1;
            let start = i;
            while i < fb.len() && fb[i].is_ascii_digit() {
                i += 1;
            }
            precision = Some(format[start..i].parse().unwrap_or(0));
        }

        if i >= fb.len() {
            break;
        }
        let spec = fb[i];
        i += 1;

        let arg = match explicit_arg {
            Some(n) => args.get(n.wrapping_sub(1)),
            None => {
                let a = args.get(next_arg);
                next_arg += 1;
                a
            }
        };
        let arg = arg.cloned().unwrap_or(PhpMixed::Null);

        let (core, numeric) = match spec {
            b'd' => (sprintf_signed_int(crate::intval(&arg), plus, space), true),
            b'u' => ((crate::intval(&arg) as u64).to_string(), true),
            b'b' => (format!("{:b}", crate::intval(&arg) as u64), true),
            b'o' => (format!("{:o}", crate::intval(&arg) as u64), true),
            b'x' => (format!("{:x}", crate::intval(&arg) as u64), true),
            b'X' => (format!("{:X}", crate::intval(&arg) as u64), true),
            b'c' => (
                String::from_utf8_lossy(&[crate::intval(&arg) as u8]).into_owned(),
                false,
            ),
            b'f' | b'F' => (
                sprintf_float(php_to_float(&arg), precision.unwrap_or(6), plus, space),
                true,
            ),
            b's' => {
                let mut s = crate::php_to_string(&arg);
                if let Some(p) = precision.filter(|&p| p < s.len()) {
                    let mut end = p;
                    while end > 0 && !s.is_char_boundary(end) {
                        end -= 1;
                    }
                    s.truncate(end);
                }
                (s, false)
            }
            // Intentionally unsupported: no Composer format string uses these, and PHP's
            // exponent formatting differs from Rust's default float formatting.
            _ => {
                panic!("Unsupported sprintf() format specifier: %{}", spec as char)
            }
        };

        out.push_str(&sprintf_pad(core, width, left, pad, numeric));
    }
    out
}

fn sprintf_signed_int(n: i64, plus: bool, space: bool) -> String {
    if n < 0 {
        n.to_string()
    } else if plus {
        format!("+{}", n)
    } else if space {
        format!(" {}", n)
    } else {
        n.to_string()
    }
}

fn sprintf_float(v: f64, precision: usize, plus: bool, space: bool) -> String {
    let negative = v.is_sign_negative() && !v.is_nan();
    let magnitude = format!("{:.*}", precision, v.abs());
    if negative {
        format!("-{}", magnitude)
    } else if plus {
        format!("+{}", magnitude)
    } else if space {
        format!(" {}", magnitude)
    } else {
        magnitude
    }
}

fn sprintf_pad(core: String, width: usize, left: bool, pad: u8, numeric: bool) -> String {
    let core_len = core.len();
    if core_len >= width {
        return core;
    }
    let fill = width - core_len;
    if left {
        // Left-justify always pads with the pad character; PHP treats a '0' flag as a space here.
        let p = if pad == b'0' { ' ' } else { pad as char };
        let mut s = core;
        for _ in 0..fill {
            s.push(p);
        }
        s
    } else if pad == b'0' && numeric {
        // Zero-padding goes after a leading sign.
        let bytes = core.as_bytes();
        let sign_len = if matches!(bytes.first(), Some(b'-' | b'+' | b' ')) {
            1
        } else {
            0
        };
        let mut s = String::with_capacity(width);
        s.push_str(&core[..sign_len]);
        for _ in 0..fill {
            s.push('0');
        }
        s.push_str(&core[sign_len..]);
        s
    } else {
        let mut s = String::with_capacity(width);
        for _ in 0..fill {
            s.push(pad as char);
        }
        s.push_str(&core);
        s
    }
}

fn php_to_float(v: &PhpMixed) -> f64 {
    match v {
        PhpMixed::Int(i) => *i as f64,
        PhpMixed::Float(f) => *f,
        PhpMixed::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        PhpMixed::String(s) => {
            // PHP's (float) cast reads the leading numeric portion of the string.
            let t = s.trim_start();
            let bytes = t.as_bytes();
            let mut end = 0;
            if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
                end += 1;
            }
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end < bytes.len() && bytes[end] == b'.' {
                end += 1;
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
            }
            if end < bytes.len() && (bytes[end] == b'e' || bytes[end] == b'E') {
                let mut e = end + 1;
                if e < bytes.len() && (bytes[e] == b'+' || bytes[e] == b'-') {
                    e += 1;
                }
                if e < bytes.len() && bytes[e].is_ascii_digit() {
                    while e < bytes.len() && bytes[e].is_ascii_digit() {
                        e += 1;
                    }
                    end = e;
                }
            }
            t[..end].parse::<f64>().unwrap_or(0.0)
        }
        _ => 0.0,
    }
}

pub fn bin2hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn ucfirst(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
    }
}

pub fn php_strip_whitespace(path: impl AsRef<std::path::Path>) -> Result<String, std::io::Error> {
    // PHP `php_strip_whitespace()` tokenizes the source and re-emits it with comments removed and
    // each run of whitespace collapsed to a single space. There is no PHP tokenizer in the shim, so
    // this is a hand-written lexer that reproduces the observable effect for the cases the class-map
    // generator depends on: it preserves single-quoted, double-quoted, backtick and heredoc/nowdoc
    // string contents verbatim while dropping `//`, `#` and `/* */` comments and squeezing
    // whitespace. PHP returns an empty string on a read failure and leaves the reason in the warning
    // text; this returns the io::Error instead so callers can report it.
    let contents = std::fs::read(path.as_ref())?;

    let b = contents;
    let n = b.len();
    let mut out: Vec<u8> = Vec::with_capacity(n);
    let mut i = 0usize;

    let push_space = |out: &mut Vec<u8>| {
        if !out.is_empty() && *out.last().unwrap() != b' ' {
            out.push(b' ');
        }
    };

    while i < n {
        let c = b[i];

        // Line comments: // ... and # ...
        if c == b'/' && i + 1 < n && b[i + 1] == b'/' {
            i += 2;
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            push_space(&mut out);
            continue;
        }
        if c == b'#' {
            i += 1;
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            push_space(&mut out);
            continue;
        }
        // Block comments: /* ... */
        if c == b'/' && i + 1 < n && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < n && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            push_space(&mut out);
            continue;
        }
        // String literals: '...', "...", `...`
        if c == b'\'' || c == b'"' || c == b'`' {
            out.push(c);
            i += 1;
            while i < n {
                let d = b[i];
                out.push(d);
                if d == b'\\' && i + 1 < n {
                    out.push(b[i + 1]);
                    i += 2;
                    continue;
                }
                i += 1;
                if d == c {
                    break;
                }
            }
            continue;
        }
        // Heredoc/Nowdoc: <<<LABEL ... LABEL
        if c == b'<' && i + 2 < n && b[i + 1] == b'<' && b[i + 2] == b'<' {
            let mut j = i + 3;
            while j < n && (b[j] == b' ' || b[j] == b'\t') {
                j += 1;
            }
            let nowdoc = j < n && b[j] == b'\'';
            let quoted = j < n && (b[j] == b'\'' || b[j] == b'"');
            if quoted {
                j += 1;
            }
            let label_start = j;
            while j < n && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            let label = b[label_start..j].to_vec();
            if !label.is_empty() {
                let _ = nowdoc;
                // Emit verbatim from the opening marker until a line whose trimmed start matches the
                // closing label.
                let body_start = i;
                let mut k = j;
                // skip to end of the opening line
                while k < n && b[k] != b'\n' {
                    k += 1;
                }
                let mut closed = n;
                while k < n {
                    k += 1; // move past '\n'
                    let line_start = k;
                    let mut m = line_start;
                    while m < n && (b[m] == b' ' || b[m] == b'\t') {
                        m += 1;
                    }
                    if b[m..].starts_with(&label) {
                        let after = m + label.len();
                        let boundary =
                            after >= n || !(b[after].is_ascii_alphanumeric() || b[after] == b'_');
                        if boundary {
                            closed = after;
                            break;
                        }
                    }
                    while k < n && b[k] != b'\n' {
                        k += 1;
                    }
                }
                out.extend_from_slice(&b[body_start..closed.min(n)]);
                i = closed.min(n);
                continue;
            }
        }
        // Whitespace runs collapse to a single space.
        if c == b' ' || c == b'\t' || c == b'\r' || c == b'\n' {
            i += 1;
            push_space(&mut out);
            continue;
        }

        out.push(c);
        i += 1;
    }

    Ok(String::from_utf8_lossy(&out).into_owned())
}

pub fn hexdec(s: &str) -> i64 {
    // PHP hexdec() ignores characters outside [0-9A-Fa-f].
    // TODO(phase-c): PHP promotes the result to float on overflow; this i64 return wraps instead.
    let mut acc: u64 = 0;
    for &b in s.as_bytes() {
        let d = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => continue,
        };
        acc = acc.wrapping_mul(16).wrapping_add(d as u64);
    }
    acc as i64
}

pub fn byte_at(s: &str, i: usize) -> u8 {
    s.as_bytes().get(i).copied().unwrap_or(0)
}

pub fn wordwrap(s: &str, width: i64, break_str: &str, cut: bool) -> String {
    // PHP throws a ValueError for either argument combination before reaching the wrapping loop.
    assert!(
        !break_str.is_empty(),
        "wordwrap(): Argument #3 ($break) must not be empty"
    );
    assert!(
        !(width == 0 && cut),
        "wordwrap(): Argument #4 ($cut) cannot be true when argument #2 ($width) is 0"
    );
    php_wordwrap(s, width, break_str, cut)
}

pub fn levenshtein(string1: &str, string2: &str) -> i64 {
    // PHP's levenshtein() is byte-based with unit insertion/deletion/replacement costs.
    let a = string1.as_bytes();
    let b = string2.as_bytes();
    let n = b.len();
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for (i, &ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n] as i64
}

pub fn number_format(
    number: f64,
    decimals: i64,
    decimal_separator: &str,
    thousands_separator: &str,
) -> String {
    let decimals = decimals.max(0) as usize;
    let negative = number < 0.0;
    let magnitude = number.abs();
    // PHP rounds half away from zero; Rust's f64::round() does the same, so round the scaled value
    // to a whole number before formatting to avoid the round-half-to-even of `{:.*}`.
    let factor = 10f64.powi(decimals as i32);
    let scaled = (magnitude * factor).round();
    let mut digits = format!("{:.0}", scaled);
    while digits.len() <= decimals {
        digits.insert(0, '0');
    }
    let split = digits.len() - decimals;
    let int_part = &digits[..split];
    let frac_part = &digits[split..];

    let mut result = String::new();
    let int_bytes = int_part.as_bytes();
    let len = int_bytes.len();
    for (idx, &b) in int_bytes.iter().enumerate() {
        if idx > 0 && (len - idx) % 3 == 0 {
            result.push_str(thousands_separator);
        }
        result.push(b as char);
    }
    if decimals > 0 {
        result.push_str(decimal_separator);
        result.push_str(frac_part);
    }
    // PHP drops the sign when the rounded value is zero.
    if negative && result.bytes().any(|b| b.is_ascii_digit() && b != b'0') {
        result.insert(0, '-');
    }
    result
}

pub fn uniqid(prefix: &str, more_entropy: bool) -> String {
    // PHP builds the id from the current time: 8 hex digits of seconds followed by 5 hex digits of
    // microseconds. With $more_entropy a '.' and a random fraction (PHP's "%08.8F") are appended.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let base = format!("{}{:08x}{:05x}", prefix, now.as_secs(), now.subsec_micros());
    if more_entropy {
        // TODO(phase-c): PHP uses its combined LCG; this uses `fastrand`, so the random suffix is
        // not reproducible against PHP (it is non-deterministic in PHP too).
        format!("{}.{:.8}", base, fastrand::f64() * 10.0)
    } else {
        base
    }
}
