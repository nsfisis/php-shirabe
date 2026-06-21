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

pub fn substr_replace(_string: &str, _replace: &str, _start: usize, _length: usize) -> String {
    todo!()
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
    todo!()
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
    todo!()
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
    todo!()
}

pub fn base64_decode(_data: &str) -> Option<Vec<u8>> {
    todo!()
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
    todo!()
}

pub fn unpack(_format: &str, _data: &[u8]) -> Option<IndexMap<String, PhpMixed>> {
    todo!()
}

pub fn sscanf(_subject: &str, _format: &str, _a: &mut i64, _b: &mut i64) -> i64 {
    todo!()
}

pub fn sprintf(_format: &str, _args: &[PhpMixed]) -> String {
    todo!()
}

pub fn strip_tags(_str: &str) -> String {
    todo!()
}

pub fn html_entity_decode(_s: &str) -> String {
    todo!()
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
    todo!()
}

pub fn addcslashes(_string: &str, _charlist: &str) -> String {
    todo!()
}

pub fn php_strip_whitespace(_path: &str) -> String {
    todo!()
}

pub fn hexdec(_s: &str) -> i64 {
    todo!()
}

pub fn byte_at(s: &str, i: usize) -> u8 {
    s.as_bytes().get(i).copied().unwrap_or(0)
}

pub fn stripcslashes(_s: &str) -> String {
    todo!()
}

pub fn wordwrap(_s: &str, _width: i64, _break_str: &str, _cut: bool) -> String {
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
    todo!()
}

pub fn uniqid(_prefix: &str, _more_entropy: bool) -> String {
    todo!()
}
