use crate::PhpMixed;
use bstr::{BStr, BString, ByteSlice, ByteVec};
use indexmap::IndexMap;
pub use shirabe_php_src::standard::string::{addcslashes, strip_tags, stripcslashes};
use shirabe_php_src::standard::string::{php_trim_mask, php_wordwrap};
use shirabe_php_src::standard::strnatcmp::strnatcmp_ex;

pub fn str_replace(
    search: impl AsRef<[u8]>,
    replace: impl AsRef<[u8]>,
    subject: impl AsRef<[u8]>,
) -> BString {
    let search = search.as_ref();
    let subject = subject.as_ref();
    // PHP returns the subject unchanged when the search string is empty, whereas bstr's `replace`
    // would insert `replace` between every byte.
    if search.is_empty() {
        return BString::from(subject);
    }

    BString::from(subject.replace(search, replace.as_ref()))
}

pub fn substr_count(haystack: impl AsRef<[u8]>, needle: impl AsRef<[u8]>) -> i64 {
    let needle = needle.as_ref();
    if needle.is_empty() {
        panic!("substr_count(): Argument #2 ($needle) cannot be empty");
    }
    // find_iter counts non-overlapping occurrences, matching PHP's substr_count.
    haystack.as_ref().find_iter(needle).count() as i64
}

// Byte-based, matching PHP's substr_replace. `length` of `None` is PHP's omitted `$length`,
// which replaces up to the end of the string.
pub fn substr_replace(
    string: impl AsRef<[u8]>,
    replace: impl AsRef<[u8]>,
    start: i64,
    length: Option<i64>,
) -> BString {
    let bytes = string.as_ref();
    let replace = replace.as_ref();
    let str_len = bytes.len() as i64;

    // A negative start counts from the end of the string, clamped to both ends of the string.
    let start = if start < 0 {
        (str_len + start).max(0)
    } else {
        start.min(str_len)
    };

    // A negative length means "stop that many bytes before the end of the string"; the result is
    // clamped to an empty range rather than becoming negative.
    let mut length = length.unwrap_or(str_len);
    if length < 0 {
        length = (str_len - start + length).max(0);
    }
    length = length.min(str_len - start);

    let start = start as usize;
    let end = start + length as usize;
    let mut out = BString::from(Vec::with_capacity(bytes.len() + replace.len()));
    out.extend_from_slice(&bytes[..start]);
    out.extend_from_slice(replace);
    out.extend_from_slice(&bytes[end..]);
    out
}

pub fn str_repeat(s: impl AsRef<[u8]>, count: usize) -> BString {
    BString::from(s.as_ref().repeat(count))
}

pub fn str_replace_array(
    search: &[impl AsRef<[u8]>],
    replace: &[impl AsRef<[u8]>],
    subject: impl AsRef<[u8]>,
) -> BString {
    // PHP's array form of str_replace replaces each search element in order with the replace
    // element at the same index, falling back to an empty string when replace is shorter.
    let mut result = BString::from(subject.as_ref());
    for (i, s) in search.iter().enumerate() {
        let r = replace.get(i).map(|r| r.as_ref()).unwrap_or(b"");
        result = str_replace(s, r, &result);
    }
    result
}

pub fn str_pad(
    input: impl AsRef<[u8]>,
    length: usize,
    pad_string: impl AsRef<[u8]>,
    pad_type: i64,
) -> BString {
    // PHP str_pad() works on bytes: it pads up to `length` bytes by repeating `pad_string`.
    let input = input.as_ref();
    let pad = pad_string.as_ref();
    let input_len = input.len();
    if length <= input_len || pad.is_empty() {
        return BString::from(input);
    }
    let make = |n: usize| -> Vec<u8> { (0..n).map(|i| pad[i % pad.len()]).collect() };
    let total = length - input_len;
    let mut out: Vec<u8> = Vec::with_capacity(length);
    match pad_type {
        STR_PAD_LEFT => {
            out.extend(make(total));
            out.extend_from_slice(input);
        }
        STR_PAD_BOTH => {
            let left = total / 2;
            out.extend(make(left));
            out.extend_from_slice(input);
            out.extend(make(total - left));
        }
        _ => {
            out.extend_from_slice(input);
            out.extend(make(total));
        }
    }
    BString::from(out)
}

pub const STR_PAD_LEFT: i64 = 0;
pub const STR_PAD_RIGHT: i64 = 1;
pub const STR_PAD_BOTH: i64 = 2;

pub fn str_split(s: impl AsRef<[u8]>, length: i64) -> Vec<BString> {
    // PHP str_split() chunks the string by bytes into pieces of `length` bytes.
    let length = length.max(1) as usize;
    let bytes = s.as_ref();
    if bytes.is_empty() {
        return vec![BString::default()];
    }
    bytes.chunks(length).map(BString::from).collect()
}

pub fn str_bitand(a: &[u8], b: &[u8]) -> BString {
    // PHP's string `&` operator: byte-wise AND, the result truncated to the shorter operand.
    a.iter().zip(b).map(|(a, b)| a & b).collect()
}

pub fn str_replace_arrays(
    search: &[impl AsRef<[u8]>],
    replace: &[impl AsRef<[u8]>],
    subject: impl AsRef<[u8]>,
) -> BString {
    str_replace_array(search, replace, subject)
}

pub fn str_replace_arr(
    search: &[impl AsRef<[u8]>],
    replace: impl AsRef<[u8]>,
    subject: impl AsRef<[u8]>,
) -> BString {
    // PHP str_replace(array, string, subject): every search element is replaced with
    // the same replacement string, applied in order.
    let replace = replace.as_ref();
    let mut result = BString::from(subject.as_ref());
    for s in search {
        result = str_replace(s, replace, &result);
    }
    result
}

pub fn strcasecmp(s1: impl AsRef<[u8]>, s2: impl AsRef<[u8]>) -> i64 {
    s1.as_ref()
        .to_ascii_lowercase()
        .cmp(&s2.as_ref().to_ascii_lowercase()) as i64
}

pub fn strpos(haystack: impl AsRef<[u8]>, needle: impl AsRef<[u8]>) -> Option<usize> {
    haystack.as_ref().find(needle.as_ref())
}

pub fn strtoupper(s: impl AsRef<[u8]>) -> BString {
    BString::from(s.as_ref().to_ascii_uppercase())
}

pub fn strlen(s: impl AsRef<[u8]>) -> i64 {
    s.as_ref().len() as i64
}

// Byte-based, matching PHP: each byte of `from` maps to the byte of `to` at the same offset, and
// the longer of the two is truncated to the shorter.
pub fn strtr(str: impl AsRef<[u8]>, from: impl AsRef<[u8]>, to: impl AsRef<[u8]>) -> BString {
    let from = from.as_ref();
    let to = to.as_ref();
    let n = from.len().min(to.len());
    let mut table: [u8; 256] = std::array::from_fn(|b| b as u8);
    for i in 0..n {
        table[from[i] as usize] = to[i];
    }
    str.as_ref().iter().map(|&b| table[b as usize]).collect()
}

pub fn strpbrk(haystack: impl AsRef<[u8]>, char_list: impl AsRef<[u8]>) -> Option<BString> {
    let set = char_list.as_ref();
    let bytes = haystack.as_ref();
    bytes
        .iter()
        .position(|b| set.contains(b))
        .map(|i| BString::from(&bytes[i..]))
}

pub fn strnatcasecmp(s1: impl AsRef<[u8]>, s2: impl AsRef<[u8]>) -> i64 {
    strnatcmp_ex(s1.as_ref(), s2.as_ref(), true)
}

pub fn strrpos(haystack: impl AsRef<[u8]>, needle: impl AsRef<[u8]>) -> Option<usize> {
    haystack.as_ref().rfind(needle.as_ref())
}

// Byte-based, matching PHP: strrev() reverses the bytes, not the characters.
pub fn strrev(s: impl AsRef<[u8]>) -> BString {
    let mut bytes = s.as_ref().to_vec();
    bytes.reverse();
    BString::from(bytes)
}

pub fn strtolower(s: impl AsRef<[u8]>) -> BString {
    BString::from(s.as_ref().to_ascii_lowercase())
}

pub fn stripos(haystack: impl AsRef<[u8]>, needle: impl AsRef<[u8]>) -> Option<usize> {
    haystack
        .as_ref()
        .to_ascii_lowercase()
        .find(needle.as_ref().to_ascii_lowercase())
}

// Byte-based, matching PHP's array form of strtr: at each position the longest
// matching key wins (insertion order breaks ties), and replacements are not
// re-scanned. Empty keys are ignored.
pub fn strtr_array<K: AsRef<[u8]>, V: AsRef<[u8]>>(
    s: impl AsRef<[u8]>,
    pairs: &IndexMap<K, V>,
) -> BString {
    let mut keys: Vec<(&K, &V)> = pairs
        .iter()
        .filter(|(k, _)| !k.as_ref().is_empty())
        .collect();
    keys.sort_by_key(|(k, _)| std::cmp::Reverse(k.as_ref().len()));

    let bytes = s.as_ref();
    let mut result: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let mut matched = false;
        for (key, value) in &keys {
            let kb = key.as_ref();
            if bytes[i..].starts_with(kb) {
                result.extend_from_slice(value.as_ref());
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
    BString::from(result)
}

pub fn strcmp(s1: impl AsRef<[u8]>, s2: impl AsRef<[u8]>) -> i64 {
    s1.as_ref().cmp(s2.as_ref()) as i64
}

pub fn strnatcmp(s1: impl AsRef<[u8]>, s2: impl AsRef<[u8]>) -> i64 {
    strnatcmp_ex(s1.as_ref(), s2.as_ref(), false)
}

pub fn strcspn(string: impl AsRef<[u8]>, characters: impl AsRef<[u8]>) -> usize {
    let set = characters.as_ref();
    string
        .as_ref()
        .iter()
        .take_while(|b| !set.contains(b))
        .count()
}

pub fn strstr(haystack: impl AsRef<[u8]>, needle: impl AsRef<[u8]>) -> Option<BString> {
    strstr3(haystack, needle, false)
}

pub fn strstr3(
    haystack: impl AsRef<[u8]>,
    needle: impl AsRef<[u8]>,
    before_needle: bool,
) -> Option<BString> {
    let haystack = haystack.as_ref();
    haystack.find(needle.as_ref()).map(|i| {
        if before_needle {
            BString::from(&haystack[..i])
        } else {
            BString::from(&haystack[i..])
        }
    })
}

/// PHP's default trim character mask: " \t\n\r\0\x0B".
const PHP_TRIM_DEFAULT_CHARS: &[u8] = b" \t\n\r\0\x0B";

fn php_trim(s: &[u8], chars: &[u8], left: bool, right: bool) -> BString {
    let mask = php_trim_mask(chars);
    let mut start = 0;
    let mut end = s.len();
    if left {
        while start < end && mask[s[start] as usize] {
            start += 1;
        }
    }
    if right {
        while end > start && mask[s[end - 1] as usize] {
            end -= 1;
        }
    }
    BString::from(&s[start..end])
}

pub fn rtrim(s: impl AsRef<[u8]>) -> BString {
    php_trim(s.as_ref(), PHP_TRIM_DEFAULT_CHARS, false, true)
}

pub fn rtrim2(s: impl AsRef<[u8]>, characters: impl AsRef<[u8]>) -> BString {
    php_trim(s.as_ref(), characters.as_ref(), false, true)
}

pub fn ltrim(s: impl AsRef<[u8]>) -> BString {
    php_trim(s.as_ref(), PHP_TRIM_DEFAULT_CHARS, true, false)
}

pub fn ltrim2(s: impl AsRef<[u8]>, characters: impl AsRef<[u8]>) -> BString {
    php_trim(s.as_ref(), characters.as_ref(), true, false)
}

pub fn trim(s: impl AsRef<[u8]>) -> BString {
    php_trim(s.as_ref(), PHP_TRIM_DEFAULT_CHARS, true, true)
}

pub fn trim2(s: impl AsRef<[u8]>, characters: impl AsRef<[u8]>) -> BString {
    php_trim(s.as_ref(), characters.as_ref(), true, true)
}

// Byte-based, matching PHP's substr. A negative start/length counts from the end.
pub fn substr(s: impl AsRef<[u8]>, start: i64, length: Option<i64>) -> BString {
    let s = s.as_ref();
    let (start, end) = php_slice_bounds(s.len() as i64, start, length);
    BString::from(&s[start..end])
}

pub fn implode(glue: impl AsRef<[u8]>, pieces: &[impl AsRef<[u8]>]) -> BString {
    BString::from(bstr::join(glue.as_ref(), pieces))
}

pub fn explode(delimiter: impl AsRef<[u8]>, string: impl AsRef<[u8]>) -> Vec<BString> {
    string
        .as_ref()
        .split_str(delimiter.as_ref())
        .map(BString::from)
        .collect()
}

fn explode_limit_impl(delimiter: &[u8], string: &[u8], limit: i64) -> Vec<BString> {
    if limit > 0 {
        string
            .splitn_str(limit as usize, delimiter)
            .map(BString::from)
            .collect()
    } else if limit == 0 {
        // PHP treats a zero limit as 1: the whole string is returned as one element.
        vec![BString::from(string)]
    } else {
        let mut parts: Vec<BString> = string.split_str(delimiter).map(BString::from).collect();
        let keep = parts.len() as i64 + limit;
        if keep <= 0 {
            Vec::new()
        } else {
            parts.truncate(keep as usize);
            parts
        }
    }
}

pub fn explode_with_limit(
    delimiter: impl AsRef<[u8]>,
    string: impl AsRef<[u8]>,
    limit: i64,
) -> Vec<BString> {
    explode_limit_impl(delimiter.as_ref(), string.as_ref(), limit)
}

pub fn explode_limit(
    delimiter: impl AsRef<[u8]>,
    string: impl AsRef<[u8]>,
    limit: i64,
) -> Vec<BString> {
    explode_limit_impl(delimiter.as_ref(), string.as_ref(), limit)
}

/// Normalizes an mbstring encoding label to a canonical spelling (e.g. `utf8` -> `UTF-8`).
fn canonical_encoding(name: &[u8]) -> BString {
    let upper = name.to_ascii_uppercase();
    match upper.replace("-", "").as_slice() {
        b"UTF8" => BString::from("UTF-8"),
        b"ASCII" | b"USASCII" => BString::from("ASCII"),
        _ => BString::from(upper),
    }
}

/// Replaces every invalid UTF-8 sequence (in `ascii_only` mode, also every non-ASCII character)
/// with mbstring's default substitute character `?`.
// TODO(php-semantics): this substitutes one `?` per maximal invalid subpart, which may not match
// how mbstring splits an invalid sequence into errors.
fn substitute_invalid(s: &[u8], ascii_only: bool) -> BString {
    let mut out = BString::from(Vec::with_capacity(s.len()));
    for (start, end, c) in s.char_indices() {
        if c == '\u{FFFD}' && &s[start..end] != "\u{FFFD}".as_bytes() || ascii_only && !c.is_ascii()
        {
            out.push(b'?');
        } else {
            out.extend_from_slice(&s[start..end]);
        }
    }
    out
}

pub fn mb_convert_encoding(
    string: impl AsRef<[u8]>,
    to_encoding: impl AsRef<[u8]>,
    from_encoding: impl AsRef<[u8]>,
) -> BString {
    let to = canonical_encoding(to_encoding.as_ref());
    let from = canonical_encoding(from_encoding.as_ref());
    // ASCII is a subset of UTF-8, so converting among ASCII/UTF-8 only substitutes the characters
    // the target (or the source) cannot hold. Other encodings need conversion tables that have not
    // been ported yet.
    match (to.as_slice(), from.as_slice()) {
        (b"UTF-8", b"UTF-8") => substitute_invalid(string.as_ref(), false),
        (b"UTF-8" | b"ASCII", b"ASCII") | (b"ASCII", b"UTF-8") => {
            substitute_invalid(string.as_ref(), true)
        }
        _ => todo!(
            "mb_convert_encoding {} -> {}",
            from.display_lossy(),
            to.display_lossy()
        ),
    }
}

// TODO(php-semantics): an invalid UTF-8 sequence counts as one character per maximal invalid
// subpart, which may not match how mbstring counts it.
pub fn mb_strlen(s: impl AsRef<[u8]>, _encoding: impl AsRef<[u8]>) -> i64 {
    s.as_ref().chars().count() as i64
}

pub fn mb_check_encoding(value: impl AsRef<[u8]>, encoding: impl AsRef<[u8]>) -> bool {
    let value = value.as_ref();
    match canonical_encoding(encoding.as_ref()).as_slice() {
        b"UTF-8" => value.is_utf8(),
        b"ASCII" => value.is_ascii(),
        // Other encodings need the mbstring validation tables, which have not been ported.
        _ => todo!(),
    }
}

pub fn mb_detect_encoding(
    s: impl AsRef<[u8]>,
    encodings: Option<Vec<BString>>,
    strict: bool,
) -> Option<BString> {
    // PHP's default detection order is ASCII then UTF-8.
    let s = s.as_ref();
    let order = encodings.unwrap_or_else(|| vec![BString::from("ASCII"), BString::from("UTF-8")]);
    // TODO(php-semantics): in non-strict mode PHP may pick a candidate the string is not valid in;
    // this always requires validity.
    let _ = strict;
    for enc in order {
        match canonical_encoding(&enc).as_slice() {
            b"ASCII" if s.is_ascii() => return Some(enc),
            b"UTF-8" if s.is_utf8() => return Some(enc),
            _ => {}
        }
    }
    None
}

pub fn mb_strwidth(s: impl AsRef<[u8]>, _encoding: Option<&BStr>) -> i64 {
    // TODO(unicode): calculate actual width
    s.as_ref().len() as i64
}

/// The byte ranges of the characters of `s`, as mbstring splits a UTF-8 string.
// TODO(php-semantics): an invalid UTF-8 sequence forms one character per maximal invalid subpart,
// which may not match how mbstring splits it.
fn utf8_char_ranges(s: &[u8]) -> Vec<(usize, usize)> {
    s.char_indices()
        .map(|(start, end, _)| (start, end))
        .collect()
}

pub fn mb_substr(
    s: impl AsRef<[u8]>,
    start: i64,
    length: Option<i64>,
    _encoding: Option<&BStr>,
) -> BString {
    // Character based, mirroring substr's byte-based offset/length handling.
    let s = s.as_ref();
    let ranges = utf8_char_ranges(s);
    let (start, end) = php_slice_bounds(ranges.len() as i64, start, length);
    if start == end {
        return BString::default();
    }
    BString::from(&s[ranges[start].0..ranges[end - 1].1])
}

pub fn mb_str_split(s: impl AsRef<[u8]>, length: i64) -> Vec<BString> {
    let s = s.as_ref();
    let length = length.max(1) as usize;
    utf8_char_ranges(s)
        .chunks(length)
        .map(|chunk| BString::from(&s[chunk[0].0..chunk[chunk.len() - 1].1]))
        .collect()
}

pub fn mb_convert_variables(
    to: impl AsRef<[u8]>,
    from: impl AsRef<[u8]>,
    vars: &mut [BString],
) -> Option<BString> {
    // Converts each variable in place from `from` to `to`, returning the source encoding (PHP
    // returns the detected source encoding; here `from` is a single named encoding).
    for v in vars.iter_mut() {
        *v = mb_convert_encoding(std::mem::take(v), to.as_ref(), from.as_ref());
    }
    Some(BString::from(from.as_ref()))
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

pub fn rawurldecode(s: impl AsRef<[u8]>) -> BString {
    let bytes = s.as_ref();
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
    BString::from(out)
}

fn push_percent_encoded(out: &mut BString, b: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push(b'%');
    out.push(HEX[(b >> 4) as usize]);
    out.push(HEX[(b & 15) as usize]);
}

pub fn rawurlencode(s: impl AsRef<[u8]>) -> BString {
    let s = s.as_ref();
    let mut out = BString::from(Vec::with_capacity(s.len()));
    for &b in s {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b);
        } else {
            push_percent_encoded(&mut out, b);
        }
    }
    out
}

pub fn urlencode(s: impl AsRef<[u8]>) -> BString {
    let s = s.as_ref();
    let mut out = BString::from(Vec::with_capacity(s.len()));
    for &b in s {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
            out.push(b);
        } else if b == b' ' {
            out.push(b'+');
        } else {
            push_percent_encoded(&mut out, b);
        }
    }
    out
}

pub fn base64_encode(data: impl AsRef<[u8]>) -> BString {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = data.as_ref();
    let mut out = BString::from(Vec::with_capacity(bytes.len().div_ceil(3) * 4));
    for chunk in bytes.chunks(3) {
        let b1 = chunk.get(1).copied();
        let b2 = chunk.get(2).copied();
        let n = (chunk[0] as u32) << 16 | (b1.unwrap_or(0) as u32) << 8 | (b2.unwrap_or(0) as u32);
        out.push(TABLE[((n >> 18) & 63) as usize]);
        out.push(TABLE[((n >> 12) & 63) as usize]);
        out.push(if b1.is_some() {
            TABLE[((n >> 6) & 63) as usize]
        } else {
            b'='
        });
        out.push(if b2.is_some() {
            TABLE[(n & 63) as usize]
        } else {
            b'='
        });
    }
    out
}

pub fn base64_decode(data: impl AsRef<[u8]>) -> Option<Vec<u8>> {
    // Non-strict mode (PHP's default $strict = false): characters outside the base64 alphabet are
    // silently skipped, and padding terminates the input.
    let data = data.as_ref();
    let mut sextets: Vec<u8> = Vec::with_capacity(data.len());
    for &b in data {
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

pub fn ctype_alnum(s: impl AsRef<[u8]>) -> bool {
    let s = s.as_ref();
    !s.is_empty() && s.iter().all(|b| b.is_ascii_alphanumeric())
}

pub fn ctype_digit(s: impl AsRef<[u8]>) -> bool {
    let s = s.as_ref();
    !s.is_empty() && s.iter().all(|b| b.is_ascii_digit())
}

pub fn ord(c: impl AsRef<[u8]>) -> i64 {
    c.as_ref().first().copied().unwrap_or(0) as i64
}

pub fn ucwords(s: impl AsRef<[u8]>) -> BString {
    // PHP's default word delimiters: space, tab, CR, LF, FF and VT.
    const DELIMITERS: &[u8] = b" \t\r\n\x0C\x0B";
    let mut capitalize_next = true;
    s.as_ref()
        .iter()
        .map(|&b| {
            let out = if capitalize_next {
                b.to_ascii_uppercase()
            } else {
                b
            };
            capitalize_next = DELIMITERS.contains(&b);
            out
        })
        .collect()
}

fn hex_digit_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// The value of a run of ASCII digits, saturating instead of overflowing.
fn parse_digits(digits: &[u8]) -> usize {
    digits.iter().fold(0usize, |acc, &d| {
        acc.saturating_mul(10).saturating_add(usize::from(d - b'0'))
    })
}

pub fn sprintf(format: impl AsRef<[u8]>, args: &[PhpMixed]) -> BString {
    let fb = format.as_ref();
    let mut out = BString::default();
    let mut i = 0;
    let mut next_arg = 0usize;
    while i < fb.len() {
        if fb[i] != b'%' {
            // Copy the literal run verbatim.
            let start = i;
            while i < fb.len() && fb[i] != b'%' {
                i += 1;
            }
            out.extend_from_slice(&fb[start..i]);
            continue;
        }
        i += 1;
        if i >= fb.len() {
            out.push(b'%');
            break;
        }
        if fb[i] == b'%' {
            out.push(b'%');
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
                explicit_arg = Some(parse_digits(&fb[i..k]));
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
                width = parse_digits(&fb[start..i]);
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
            precision = Some(parse_digits(&fb[start..i]));
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

        let (core, numeric): (BString, bool) = match spec {
            b'd' => (
                sprintf_signed_int(crate::intval(&arg), plus, space).into(),
                true,
            ),
            b'u' => ((crate::intval(&arg) as u64).to_string().into(), true),
            b'b' => (format!("{:b}", crate::intval(&arg) as u64).into(), true),
            b'o' => (format!("{:o}", crate::intval(&arg) as u64).into(), true),
            b'x' => (format!("{:x}", crate::intval(&arg) as u64).into(), true),
            b'X' => (format!("{:X}", crate::intval(&arg) as u64).into(), true),
            b'c' => (BString::from(vec![crate::intval(&arg) as u8]), false),
            b'f' | b'F' => (
                sprintf_float(php_to_float(&arg), precision.unwrap_or(6), plus, space).into(),
                true,
            ),
            b's' => {
                let mut s = crate::php_to_string(&arg);
                if let Some(p) = precision.filter(|&p| p < s.len()) {
                    s.truncate(p);
                }
                (s, false)
            }
            // Intentionally unsupported: no Composer format string uses these, and PHP's
            // exponent formatting differs from Rust's default float formatting.
            _ => {
                panic!("Unsupported sprintf() format specifier: %{}", spec as char)
            }
        };

        out.extend_from_slice(&sprintf_pad(core, width, left, pad, numeric));
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

fn sprintf_pad(core: BString, width: usize, left: bool, pad: u8, numeric: bool) -> BString {
    let core_len = core.len();
    if core_len >= width {
        return core;
    }
    let fill = width - core_len;
    let mut s = BString::from(Vec::with_capacity(width));
    if left {
        // Left-justify always pads with the pad character; PHP treats a '0' flag as a space here.
        let p = if pad == b'0' { b' ' } else { pad };
        s.extend_from_slice(&core);
        s.resize(width, p);
    } else if pad == b'0' && numeric {
        // Zero-padding goes after a leading sign.
        let sign_len = if matches!(core.first(), Some(b'-' | b'+' | b' ')) {
            1
        } else {
            0
        };
        s.extend_from_slice(&core[..sign_len]);
        s.resize(sign_len + fill, b'0');
        s.extend_from_slice(&core[sign_len..]);
    } else {
        s.resize(fill, pad);
        s.extend_from_slice(&core);
    }
    s
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
            let bytes = s.trim_start();
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
            // The numeric prefix is ASCII.
            bytes[..end]
                .iter()
                .map(|&b| b as char)
                .collect::<String>()
                .parse::<f64>()
                .unwrap_or(0.0)
        }
        _ => 0.0,
    }
}

pub fn bin2hex(data: impl AsRef<[u8]>) -> BString {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    data.as_ref()
        .iter()
        .flat_map(|&b| [HEX[(b >> 4) as usize], HEX[(b & 15) as usize]])
        .collect()
}

pub fn ucfirst(s: impl AsRef<[u8]>) -> BString {
    let mut out = BString::from(s.as_ref());
    if let Some(first) = out.first_mut() {
        *first = first.to_ascii_uppercase();
    }
    out
}

pub fn php_strip_whitespace(path: impl AsRef<std::path::Path>) -> Result<BString, std::io::Error> {
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

    Ok(BString::from(out))
}

pub fn hexdec(s: impl AsRef<[u8]>) -> i64 {
    // PHP hexdec() ignores characters outside [0-9A-Fa-f].
    // TODO(php-semantics): PHP promotes the result to float on overflow; this i64 return wraps instead.
    let mut acc: u64 = 0;
    for &b in s.as_ref() {
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

pub fn byte_at(s: impl AsRef<[u8]>, i: usize) -> u8 {
    s.as_ref().get(i).copied().unwrap_or(0)
}

pub fn wordwrap(
    s: impl AsRef<[u8]>,
    width: i64,
    break_str: impl AsRef<[u8]>,
    cut: bool,
) -> BString {
    let break_str = break_str.as_ref();
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

pub fn levenshtein(string1: impl AsRef<[u8]>, string2: impl AsRef<[u8]>) -> i64 {
    // PHP's levenshtein() is byte-based with unit insertion/deletion/replacement costs.
    let a = string1.as_ref();
    let b = string2.as_ref();
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
    decimal_separator: impl AsRef<[u8]>,
    thousands_separator: impl AsRef<[u8]>,
) -> BString {
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
    let int_part = &digits.as_bytes()[..split];
    let frac_part = &digits.as_bytes()[split..];

    let mut result = BString::default();
    let len = int_part.len();
    for (idx, &b) in int_part.iter().enumerate() {
        if idx > 0 && (len - idx) % 3 == 0 {
            result.extend_from_slice(thousands_separator.as_ref());
        }
        result.push(b);
    }
    if decimals > 0 {
        result.extend_from_slice(decimal_separator.as_ref());
        result.extend_from_slice(frac_part);
    }
    // PHP drops the sign when the rounded value is zero.
    if negative && digits.bytes().any(|b| b != b'0') {
        result.insert(0, b'-');
    }
    result
}

pub fn uniqid(prefix: impl AsRef<[u8]>, more_entropy: bool) -> BString {
    // PHP builds the id from the current time: 8 hex digits of seconds followed by 5 hex digits of
    // microseconds. With $more_entropy a '.' and a random fraction (PHP's "%08.8F") are appended.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let mut out = BString::from(prefix.as_ref());
    out.push_str(format!("{:08x}{:05x}", now.as_secs(), now.subsec_micros()));
    if more_entropy {
        // TODO(php-semantics): PHP uses its combined LCG; this uses `fastrand`, so the random suffix is
        // not reproducible against PHP (it is non-deterministic in PHP too).
        out.push_str(format!(".{:.8}", fastrand::f64() * 10.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{str_bitand, substr_replace};

    #[test]
    fn substr_replace_matches_php() {
        // Expected values are the output of PHP 8.5.9's substr_replace().
        assert_eq!(substr_replace("Hello World", "XX", 0, None), "XX");
        assert_eq!(substr_replace("Hello World", "XX", 6, None), "Hello XX");
        assert_eq!(substr_replace("Hello World", "XX", 6, Some(5)), "Hello XX");
        assert_eq!(
            substr_replace("Hello World", "XX", 6, Some(100)),
            "Hello XX"
        );
        assert_eq!(
            substr_replace("Hello World", "XX", 20, Some(3)),
            "Hello WorldXX"
        );
        assert_eq!(
            substr_replace("Hello World", "XX", 11, Some(3)),
            "Hello WorldXX"
        );
        assert_eq!(substr_replace("Hello World", "XX", -5, None), "Hello XX");
        assert_eq!(
            substr_replace("Hello World", "XX", -5, Some(2)),
            "Hello XXrld"
        );
        assert_eq!(
            substr_replace("Hello World", "XX", -100, Some(3)),
            "XXlo World"
        );
        assert_eq!(substr_replace("Hello World", "XX", -100, None), "XX");
        assert_eq!(substr_replace("Hello World", "XX", 0, Some(-5)), "XXWorld");
        assert_eq!(
            substr_replace("Hello World", "XX", 0, Some(-100)),
            "XXHello World"
        );
        assert_eq!(substr_replace("Hello World", "XX", 3, Some(-3)), "HelXXrld");
        assert_eq!(
            substr_replace("Hello World", "XX", -5, Some(-2)),
            "Hello XXld"
        );
        assert_eq!(
            substr_replace("Hello World", "XX", -5, Some(-100)),
            "Hello XXWorld"
        );
        assert_eq!(
            substr_replace("Hello World", "XX", 6, Some(0)),
            "Hello XXWorld"
        );
        assert_eq!(
            substr_replace("Hello World", "XX", 0, Some(0)),
            "XXHello World"
        );
        assert_eq!(substr_replace("", "XX", 0, None), "XX");
        assert_eq!(substr_replace("", "XX", 5, Some(3)), "XX");
        assert_eq!(substr_replace("", "XX", -5, Some(-3)), "XX");
        assert_eq!(substr_replace("Hello World", "", 0, Some(5)), " World");
        assert_eq!(substr_replace("Hello World", "", -5, None), "Hello ");
        assert_eq!(substr_replace("abc", "X", 1, Some(-1)), "aXc");
        assert_eq!(substr_replace("abc", "X", 2, Some(-2)), "abXc");
        assert_eq!(substr_replace("abc", "X", -1, Some(-1)), "abXc");
        assert_eq!(substr_replace("abc", "X", 3, Some(0)), "abcX");
        assert_eq!(substr_replace("abc", "X", 3, None), "abcX");
        // Splitting a multi-byte sequence keeps both halves as raw bytes.
        assert_eq!(
            substr_replace("é", "X", 1, Some(0)),
            b"\xc3X\xa9".as_slice()
        );
    }

    #[test]
    fn str_bitand_matches_php() {
        // Expected values are the output of PHP 8.5.9's `&` operator on strings.
        assert_eq!(str_bitand(b"\xc3", b"\xf0"), b"\xc0".as_slice());
        assert_eq!(str_bitand(b"abc", b"\xdf\xdf"), "AB");
    }
}
