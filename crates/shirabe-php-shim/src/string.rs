use crate::PhpMixed;
use indexmap::IndexMap;

pub fn str_replace(search: &str, replace: &str, subject: &str) -> String {
    // PHP returns the subject unchanged when the search string is empty, whereas Rust's
    // `str::replace` would insert `replace` between every character.
    if search.is_empty() {
        return subject.to_string();
    }

    subject.replace(search, replace)
}

pub fn str_contains(_haystack: &str, _needle: &str) -> bool {
    _haystack.contains(_needle)
}

pub fn str_starts_with(_haystack: &str, _needle: &str) -> bool {
    _haystack.starts_with(_needle)
}

pub fn str_ends_with(_haystack: &str, _needle: &str) -> bool {
    _haystack.ends_with(_needle)
}

pub fn substr_count(haystack: &str, needle: &str) -> i64 {
    if needle.is_empty() {
        panic!("substr_count(): Argument #2 ($needle) cannot be empty");
    }
    // str::matches counts non-overlapping occurrences, matching PHP's substr_count.
    haystack.matches(needle).count() as i64
}

// Byte-based, matching PHP's substr_replace.
// TODO(phase-d): PHP accepts negative $start/$length (counting from the end); this signature takes
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

pub fn str_repeat(_s: &str, _count: usize) -> String {
    _s.repeat(_count)
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

pub fn str_pad(_input: &str, _length: usize, _pad_string: &str, _pad_type: i64) -> String {
    // PHP str_pad() works on bytes: it pads up to `length` bytes by repeating `pad_string`.
    let input_len = _input.len();
    if _length <= input_len || _pad_string.is_empty() {
        return _input.to_string();
    }
    let pad = _pad_string.as_bytes();
    let make = |n: usize| -> Vec<u8> { (0..n).map(|i| pad[i % pad.len()]).collect() };
    let total = _length - input_len;
    let mut out: Vec<u8> = Vec::with_capacity(_length);
    match _pad_type {
        STR_PAD_LEFT => {
            out.extend(make(total));
            out.extend_from_slice(_input.as_bytes());
        }
        STR_PAD_BOTH => {
            let left = total / 2;
            out.extend(make(left));
            out.extend_from_slice(_input.as_bytes());
            out.extend(make(total - left));
        }
        _ => {
            out.extend_from_slice(_input.as_bytes());
            out.extend(make(total));
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub const STR_PAD_LEFT: i64 = 0;
pub const STR_PAD_RIGHT: i64 = 1;
pub const STR_PAD_BOTH: i64 = 2;

pub fn str_split(_s: &str, _length: i64) -> Vec<String> {
    // PHP str_split() chunks the string by bytes into pieces of `length` bytes.
    let length = _length.max(1) as usize;
    let bytes = _s.as_bytes();
    if bytes.is_empty() {
        return vec![String::new()];
    }
    bytes
        .chunks(length)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect()
}

pub fn str_bitand(_a: &str, _b: &str) -> String {
    // PHP's string `&` operator: byte-wise AND, the result truncated to the shorter operand.
    let a = _a.as_bytes();
    let b = _b.as_bytes();
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

pub fn strcasecmp(_s1: &str, _s2: &str) -> i64 {
    _s1.to_ascii_lowercase().cmp(&_s2.to_ascii_lowercase()) as i64
}

pub fn strpos(_haystack: &str, _needle: &str) -> Option<usize> {
    _haystack.find(_needle)
}

pub fn strtoupper(_s: &str) -> String {
    _s.to_ascii_uppercase()
}

pub fn strlen(_s: &str) -> i64 {
    _s.len() as i64
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

pub fn strrpos(_haystack: &str, _needle: &str) -> Option<usize> {
    _haystack.rfind(_needle)
}

pub fn strtolower(_s: &str) -> String {
    _s.to_ascii_lowercase()
}

pub fn stripos(_haystack: &str, _needle: &str) -> Option<usize> {
    _haystack
        .to_ascii_lowercase()
        .find(_needle.to_ascii_lowercase().as_str())
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

pub fn strcmp(_s1: &str, _s2: &str) -> i64 {
    _s1.cmp(_s2) as i64
}

pub fn strnatcmp(s1: &str, s2: &str) -> i64 {
    strnatcmp_ex(s1.as_bytes(), s2.as_bytes(), false)
}

// Port of PHP's strnatcmp_ex (ext/standard/strnatcmp.c). Operating on byte
// slices, an out-of-range index reads as 0, reproducing the NUL terminator that
// the C implementation relies on.
fn strnatcmp_ex(a: &[u8], b: &[u8], fold_case: bool) -> i64 {
    let a_len = a.len();
    let b_len = b.len();
    if a_len == 0 || b_len == 0 {
        return match a_len.cmp(&b_len) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Greater => 1,
            std::cmp::Ordering::Equal => 0,
        };
    }

    let mut ap = 0usize;
    let mut bp = 0usize;
    let mut leading = true;
    loop {
        let mut ca = natcmp_at(a, ap);
        let mut cb = natcmp_at(b, bp);

        // Skip over leading zeros.
        while leading && ca == b'0' && natcmp_at(a, ap + 1).is_ascii_digit() {
            ap += 1;
            ca = natcmp_at(a, ap);
        }
        while leading && cb == b'0' && natcmp_at(b, bp + 1).is_ascii_digit() {
            bp += 1;
            cb = natcmp_at(b, bp);
        }
        leading = false;

        // Skip consecutive whitespace.
        while natcmp_is_space(ca) {
            ap += 1;
            ca = natcmp_at(a, ap);
        }
        while natcmp_is_space(cb) {
            bp += 1;
            cb = natcmp_at(b, bp);
        }

        // Process a run of digits.
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let fractional = ca == b'0' || cb == b'0';
            let result = if fractional {
                natcmp_compare_left(a, &mut ap, b, &mut bp)
            } else {
                natcmp_compare_right(a, &mut ap, b, &mut bp)
            };
            if result != 0 {
                return result;
            }
        }

        if ap == a_len && bp == b_len {
            return 0;
        } else if ap == a_len {
            return -1;
        } else if bp == b_len {
            return 1;
        }

        if fold_case {
            ca = natcmp_at(a, ap).to_ascii_uppercase();
            cb = natcmp_at(b, bp).to_ascii_uppercase();
        } else {
            ca = natcmp_at(a, ap);
            cb = natcmp_at(b, bp);
        }

        if ca < cb {
            return -1;
        } else if ca > cb {
            return 1;
        }

        ap += 1;
        bp += 1;
    }
}

fn natcmp_at(s: &[u8], i: usize) -> u8 {
    if i < s.len() { s[i] } else { 0 }
}

fn natcmp_is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

// Compare two right-aligned numbers: the longest run of digits wins; failing
// that, the first differing digit decides, but only once magnitudes are known
// equal (tracked in `bias`).
fn natcmp_compare_right(a: &[u8], ap: &mut usize, b: &[u8], bp: &mut usize) -> i64 {
    let mut bias = 0i64;
    loop {
        let ca = natcmp_at(a, *ap);
        let cb = natcmp_at(b, *bp);
        let a_digit = ca.is_ascii_digit();
        let b_digit = cb.is_ascii_digit();
        if !a_digit && !b_digit {
            return bias;
        } else if !a_digit {
            return -1;
        } else if !b_digit {
            return 1;
        } else if ca < cb {
            if bias == 0 {
                bias = -1;
            }
        } else if ca > cb {
            if bias == 0 {
                bias = 1;
            }
        }
        *ap += 1;
        *bp += 1;
    }
}

// Compare two left-aligned numbers: the first differing digit decides.
fn natcmp_compare_left(a: &[u8], ap: &mut usize, b: &[u8], bp: &mut usize) -> i64 {
    loop {
        let ca = natcmp_at(a, *ap);
        let cb = natcmp_at(b, *bp);
        let a_digit = ca.is_ascii_digit();
        let b_digit = cb.is_ascii_digit();
        if !a_digit && !b_digit {
            return 0;
        } else if !a_digit {
            return -1;
        } else if !b_digit {
            return 1;
        } else if ca < cb {
            return -1;
        } else if ca > cb {
            return 1;
        }
        *ap += 1;
        *bp += 1;
    }
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

/// PHP's default trim character mask: " \t\n\r\0\x0B".
const PHP_TRIM_DEFAULT_CHARS: &[u8] = b" \t\n\r\0\x0B";

/// Build the set of bytes to strip from a PHP trim `$characters` argument,
/// expanding `a..b` range syntax as PHP does.
fn php_trim_mask(chars: &[u8]) -> [bool; 256] {
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

pub fn implode(_glue: &str, _pieces: &[String]) -> String {
    _pieces.join(_glue)
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

pub fn mb_convert_encoding(_string: Vec<u8>, _to_encoding: &str, _from_encoding: &str) -> String {
    let to = canonical_encoding(_to_encoding);
    let from = canonical_encoding(_from_encoding);
    // ASCII is a subset of UTF-8, so converting among ASCII/UTF-8 is a byte-level no-op. Other
    // encodings need conversion tables that have not been ported yet.
    if matches!(to.as_str(), "UTF-8" | "ASCII") && matches!(from.as_str(), "UTF-8" | "ASCII") {
        return String::from_utf8_lossy(&_string).into_owned();
    }
    todo!("mb_convert_encoding {} -> {}", from, to)
}

pub fn mb_strlen(s: &str, _encoding: &str) -> i64 {
    // `s` is valid UTF-8, so the character count is its number of code points.
    s.chars().count() as i64
}

pub fn mb_check_encoding(_value: &str, _encoding: &str) -> bool {
    match _encoding.to_ascii_uppercase().replace('-', "").as_str() {
        // A Rust &str is, by construction, valid UTF-8.
        "UTF8" => true,
        _ => todo!(),
    }
}

pub fn mb_detect_encoding(
    _s: &str,
    _encodings: Option<Vec<String>>,
    _strict: bool,
) -> Option<String> {
    // PHP's default detection order is ASCII then UTF-8. `_s` is already valid UTF-8, so detection
    // reduces to: pure-ASCII content matches "ASCII", anything else matches "UTF-8".
    let order = _encodings.unwrap_or_else(|| vec!["ASCII".to_string(), "UTF-8".to_string()]);
    for enc in order {
        match canonical_encoding(&enc).as_str() {
            "ASCII" if _s.is_ascii() => return Some(enc),
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

pub fn mb_convert_variables(_to: &str, _from: &str, _vars: &mut Vec<String>) -> Option<String> {
    // Converts each variable in place from `_from` to `_to`, returning the source encoding (PHP
    // returns the detected source encoding; here `_from` is a single named encoding).
    for v in _vars.iter_mut() {
        *v = mb_convert_encoding(std::mem::take(v).into_bytes(), _to, _from);
    }
    Some(_from.to_string())
}

pub fn iconv(_in_charset: &str, _out_charset: &str, _string: &str) -> Option<String> {
    let from = canonical_encoding(_in_charset);
    // The output charset may carry a "//TRANSLIT" / "//IGNORE" suffix.
    let to_base = _out_charset.split("//").next().unwrap_or(_out_charset);
    let to = canonical_encoding(to_base);
    // ASCII is a subset of UTF-8, so any conversion among ASCII/UTF-8 that targets UTF-8 is a
    // byte-level no-op.
    if to == "UTF-8" && matches!(from.as_str(), "UTF-8" | "ASCII") {
        return Some(_string.to_string());
    }
    if to == "ASCII" && from == "ASCII" {
        return Some(_string.to_string());
    }
    // TODO(phase-d): general iconv conversion needs encoding tables and //TRANSLIT///IGNORE handling
    // for non-UTF-8 targets, which have not been ported yet.
    todo!("iconv {} -> {}", from, to)
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
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) =
                (hex_digit_value(bytes[i + 1]), hex_digit_value(bytes[i + 2]))
            {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
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

pub fn base64_encode(_data: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = _data.as_bytes();
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

pub fn base64_decode(_data: &str) -> Option<Vec<u8>> {
    // Non-strict mode (PHP's default $strict = false): characters outside the base64 alphabet are
    // silently skipped, and padding terminates the input.
    let mut sextets: Vec<u8> = Vec::with_capacity(_data.len());
    for &b in _data.as_bytes() {
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

pub fn ctype_alnum(_s: &str) -> bool {
    !_s.is_empty() && _s.bytes().all(|b| b.is_ascii_alphanumeric())
}

pub fn ctype_digit(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

pub fn ord(_c: &str) -> i64 {
    _c.as_bytes().first().copied().unwrap_or(0) as i64
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

pub fn pack(_format: &str, _values: &[PhpMixed]) -> Vec<u8> {
    let fb = _format.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut vi = 0usize;
    let mut i = 0;
    while i < fb.len() {
        let code = fb[i];
        i += 1;
        // Repeat count: a number, '*' (consume the rest), or implicitly 1.
        let mut repeat: usize = 1;
        let mut star = false;
        if i < fb.len() && fb[i] == b'*' {
            star = true;
            i += 1;
        } else {
            let start = i;
            while i < fb.len() && fb[i].is_ascii_digit() {
                i += 1;
            }
            if i > start {
                repeat = _format[start..i].parse().unwrap_or(1);
            }
        }
        match code {
            b'C' | b'c' | b'n' | b'v' | b'N' | b'V' => {
                let count = if star {
                    _values.len().saturating_sub(vi)
                } else {
                    repeat
                };
                for _ in 0..count {
                    let val = crate::intval(_values.get(vi).unwrap_or(&PhpMixed::Null));
                    vi += 1;
                    match code {
                        b'C' | b'c' => out.push(val as u8),
                        b'n' => out.extend_from_slice(&(val as u16).to_be_bytes()),
                        b'v' => out.extend_from_slice(&(val as u16).to_le_bytes()),
                        b'N' => out.extend_from_slice(&(val as u32).to_be_bytes()),
                        b'V' => out.extend_from_slice(&(val as u32).to_le_bytes()),
                        _ => unreachable!(),
                    }
                }
            }
            b'a' | b'A' => {
                let s = crate::php_to_string(_values.get(vi).unwrap_or(&PhpMixed::Null));
                vi += 1;
                let bytes = s.as_bytes();
                let len = if star { bytes.len() } else { repeat };
                let pad = if code == b'A' { b' ' } else { 0 };
                for j in 0..len {
                    out.push(bytes.get(j).copied().unwrap_or(pad));
                }
            }
            _ => {
                // TODO(phase-d): only the C/c/n/v/N/V/a/A pack format codes are ported; the
                // machine-size and floating-point codes are not.
                todo!("pack format code {}", code as char)
            }
        }
    }
    out
}

pub fn unpack(_format: &str, _data: &[u8]) -> Option<IndexMap<String, PhpMixed>> {
    let mut result = IndexMap::new();
    let mut offset = 0usize;
    for group in _format.split('/') {
        if group.is_empty() {
            continue;
        }
        let gb = group.as_bytes();
        let code = gb[0];
        let mut j = 1;
        let mut repeat: usize = 1;
        let mut star = false;
        if j < gb.len() && gb[j] == b'*' {
            star = true;
            j += 1;
        } else {
            let start = j;
            while j < gb.len() && gb[j].is_ascii_digit() {
                j += 1;
            }
            if j > start {
                repeat = group[start..j].parse().unwrap_or(1);
            }
        }
        let name = &group[j..];
        // `i`/`I` are the native int (4 bytes on the LP64 targets in use); `s`/`S` are the native
        // short (2 bytes).
        let size = match code {
            b'C' | b'c' => 1,
            b'n' | b'v' | b's' | b'S' => 2,
            b'N' | b'V' | b'i' | b'I' => 4,
            _ => {
                // TODO(phase-d): only C/c/n/v/N/V/s/S/i/I unpack format codes are ported; the
                // machine-size long/quad and floating-point codes are not.
                todo!("unpack format code {}", code as char)
            }
        };
        let count = if star {
            _data.len().saturating_sub(offset) / size
        } else {
            repeat
        };
        for idx in 0..count {
            if offset + size > _data.len() {
                break;
            }
            let chunk = &_data[offset..offset + size];
            offset += size;
            let value: i64 = match code {
                b'C' => chunk[0] as i64,
                b'c' => chunk[0] as i8 as i64,
                b'n' => u16::from_be_bytes([chunk[0], chunk[1]]) as i64,
                b'v' => u16::from_le_bytes([chunk[0], chunk[1]]) as i64,
                b's' => i16::from_ne_bytes([chunk[0], chunk[1]]) as i64,
                b'S' => u16::from_ne_bytes([chunk[0], chunk[1]]) as i64,
                b'N' => u32::from_be_bytes(chunk.try_into().unwrap()) as i64,
                b'V' => u32::from_le_bytes(chunk.try_into().unwrap()) as i64,
                b'i' => i32::from_ne_bytes(chunk.try_into().unwrap()) as i64,
                b'I' => u32::from_ne_bytes(chunk.try_into().unwrap()) as i64,
                _ => unreachable!(),
            };
            // PHP keys: "name" for a single element; "name1", "name2", ... for repeats; the 1-based
            // index alone when the name is empty.
            let key = if star || repeat > 1 {
                if name.is_empty() {
                    (idx + 1).to_string()
                } else {
                    format!("{}{}", name, idx + 1)
                }
            } else if name.is_empty() {
                "1".to_string()
            } else {
                name.to_string()
            };
            result.insert(key, PhpMixed::Int(value));
        }
    }
    Some(result)
}

pub fn sscanf(_subject: &str, _format: &str, _a: &mut i64, _b: &mut i64) -> i64 {
    // TODO(phase-d): a general sscanf format-string parser is not ported; this specialized two-int
    // overload has no current callers.
    todo!()
}

pub fn sprintf(_format: &str, _args: &[PhpMixed]) -> String {
    let fb = _format.as_bytes();
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
            out.push_str(&_format[start..i]);
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
                explicit_arg = _format[i..k].parse::<usize>().ok();
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
                width = _format[start..i].parse().unwrap_or(0);
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
            precision = Some(_format[start..i].parse().unwrap_or(0));
        }

        if i >= fb.len() {
            break;
        }
        let spec = fb[i];
        i += 1;

        let arg = match explicit_arg {
            Some(n) => _args.get(n.wrapping_sub(1)),
            None => {
                let a = _args.get(next_arg);
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
            // TODO(phase-d): %e/%E/%g/%G are not ported; their exponent formatting differs from
            // Rust's default float formatting and an exact PHP match has not been implemented.
            b'e' | b'E' | b'g' | b'G' => todo!("sprintf conversion %{}", spec as char),
            _ => todo!("sprintf conversion %{}", spec as char),
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

// Port of PHP's php_strip_tags without the allowed-tags parameter (which this signature omits).
// State: 0 = text, 1 = inside a tag, 2 = inside an HTML comment, 3 = inside `<? ... ?>` / `<! ...`.
// TODO(phase-d): this omits allowed-tags handling and the tag-depth counter, so it can diverge from
// PHP on malformed markup (unterminated comments/quotes, nested `<`).
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
                        _ => {}
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

pub fn html_entity_decode(_s: &str) -> String {
    // TODO(phase-d): only numeric entities and the most common named entities (the HTML 4.01 markup
    // set PHP enables by default) are decoded; the full named-entity table is not ported.
    let chars: Vec<char> = _s.chars().collect();
    let mut out = String::with_capacity(_s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '&' {
            if let Some(rel) = chars[i + 1..].iter().position(|&c| c == ';') {
                let entity: String = chars[i + 1..i + 1 + rel].iter().collect();
                if let Some(decoded) = decode_html_entity(&entity) {
                    out.push_str(&decoded);
                    i = i + 1 + rel + 1;
                    continue;
                }
            }
            out.push('&');
            i += 1;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn decode_html_entity(entity: &str) -> Option<String> {
    if let Some(num) = entity.strip_prefix('#') {
        let code = if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            num.parse::<u32>().ok()?
        };
        return char::from_u32(code).map(|c| c.to_string());
    }
    // PHP's default flags (ENT_QUOTES | ENT_HTML401) do not include the XML-only `apos`.
    let c = match entity {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "nbsp" => '\u{00A0}',
        _ => return None,
    };
    Some(c.to_string())
}

pub fn bin2hex(_data: &[u8]) -> String {
    _data.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn ucfirst(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
    }
}

pub fn chr(_value: u8) -> String {
    // TODO(phase-d): PHP chr() yields a single raw byte; for values >= 0x80 that byte is not valid
    // UTF-8, so storing it in a Rust String forces a lossy substitution.
    String::from_utf8_lossy(&[_value]).into_owned()
}

// Port of PHP's addcslashes: every byte that falls in the (range-expanded) charlist is
// backslash-escaped, with non-printable bytes rendered as the C escape or a three-digit octal.
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

pub fn php_strip_whitespace(_path: &str) -> String {
    // TODO(phase-d): PHP strips comments and redundant whitespace using the PHP tokenizer; no PHP
    // tokenizer is available in the shim.
    todo!()
}

pub fn hexdec(_s: &str) -> i64 {
    // PHP hexdec() ignores characters outside [0-9A-Fa-f].
    // TODO(phase-d): PHP promotes the result to float on overflow; this i64 return wraps instead.
    let mut acc: u64 = 0;
    for &b in _s.as_bytes() {
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

// Port of PHP's stripcslashes: the inverse of addcslashes, decoding C escape sequences including
// octal (\ooo) and hex (\xHH).
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

pub fn wordwrap(_s: &str, _width: i64, _break_str: &str, _cut: bool) -> String {
    // TODO(phase-d): an exact byte-for-byte port of php_string_wordwrap (with its lastspace/cut
    // bookkeeping) is intricate; left unported as it has no current callers.
    todo!()
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
    _number: f64,
    _decimals: i64,
    _decimal_separator: &str,
    _thousands_separator: &str,
) -> String {
    let decimals = _decimals.max(0) as usize;
    let negative = _number < 0.0;
    let magnitude = _number.abs();
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
            result.push_str(_thousands_separator);
        }
        result.push(b as char);
    }
    if decimals > 0 {
        result.push_str(_decimal_separator);
        result.push_str(frac_part);
    }
    // PHP drops the sign when the rounded value is zero.
    if negative && result.bytes().any(|b| b.is_ascii_digit() && b != b'0') {
        result.insert(0, '-');
    }
    result
}

pub fn uniqid(_prefix: &str, _more_entropy: bool) -> String {
    // PHP builds the id from the current time: 8 hex digits of seconds followed by 5 hex digits of
    // microseconds. With $more_entropy a '.' and a random fraction (PHP's "%08.8F") are appended.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let base = format!(
        "{}{:08x}{:05x}",
        _prefix,
        now.as_secs(),
        now.subsec_micros()
    );
    if _more_entropy {
        // TODO(phase-d): PHP uses its combined LCG; this uses `fastrand`, so the random suffix is
        // not reproducible against PHP (it is non-deterministic in PHP too).
        format!("{}.{:.8}", base, fastrand::f64() * 10.0)
    } else {
        base
    }
}
