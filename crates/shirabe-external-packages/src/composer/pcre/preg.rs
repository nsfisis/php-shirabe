//! ref: composer/vendor/composer/pcre/src/Preg.php
//!
//! The following two exception classes are intentionally not ported:
//!
//! - `PcreException`: thrown when a `preg_*()` call returns false. Composer never feeds a pattern
//!   that fails to compile at runtime, so such a failure would be a programming error rather than
//!   a recoverable condition; they panic instead.
//! - `UnexpectedNullMatchException`: thrown by the `Preg::*StrictGroups()` variants when a capture
//!   group did not participate. Those variants were dropped because Rust's `Option` already
//!   distinguishes participating from non-participating groups.

use indexmap::IndexMap;

pub const PREG_PATTERN_ORDER: i64 = 1;
pub const PREG_SET_ORDER: i64 = 2;
pub const PREG_OFFSET_CAPTURE: i64 = 256;
pub const PREG_UNMATCHED_AS_NULL: i64 = 512;
pub const PREG_SPLIT_NO_EMPTY: i64 = 1;
pub const PREG_SPLIT_DELIM_CAPTURE: i64 = 2;
pub const PREG_SPLIT_OFFSET_CAPTURE: i64 = 4;
pub const PREG_GREP_INVERT: i64 = 1;

#[derive(Debug)]
pub struct Preg;

impl Preg {
    pub fn match3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut IndexMap<CaptureKey, String>>,
    ) -> anyhow::Result<bool> {
        Self::match5(pattern, subject, matches, 0, 0)
    }

    pub fn match5(
        pattern: &str,
        subject: &str,
        matches: Option<&mut IndexMap<CaptureKey, String>>,
        flags: i64,
        offset: usize,
    ) -> anyhow::Result<bool> {
        Self::check_offset_capture(flags, "matchWithOffsets");

        let mut internal: IndexMap<CaptureKey, Option<String>> = IndexMap::new();
        let result = preg_match(
            pattern,
            subject,
            Some(&mut internal),
            flags | PREG_UNMATCHED_AS_NULL,
            offset,
        )
        .unwrap_or_else(|| invalid_regex());

        if let Some(out) = matches {
            *out = drop_null_matches(internal);
        }

        Ok(result == 1)
    }

    pub fn match_all(pattern: &str, subject: &str) -> anyhow::Result<usize> {
        Self::match_all5(pattern, subject, None, 0, 0)
    }

    pub fn match_all3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut IndexMap<CaptureKey, Vec<String>>>,
    ) -> anyhow::Result<usize> {
        Self::match_all5(pattern, subject, matches, 0, 0)
    }

    pub fn match_all5(
        pattern: &str,
        subject: &str,
        matches: Option<&mut IndexMap<CaptureKey, Vec<String>>>,
        flags: i64,
        offset: usize,
    ) -> anyhow::Result<usize> {
        Self::check_offset_capture(flags, "matchAllWithOffsets");
        Self::check_set_order(flags);

        let mut internal: IndexMap<CaptureKey, Vec<Option<String>>> = IndexMap::new();
        let result = preg_match_all(
            pattern,
            subject,
            Some(&mut internal),
            flags | PREG_UNMATCHED_AS_NULL,
            offset,
        )
        .unwrap_or_else(|| invalid_regex());

        if let Some(out) = matches {
            *out = null_to_empty_match_all(internal);
        }

        Ok(result as usize)
    }

    pub fn match_all_with_offsets5(
        pattern: &str,
        subject: &str,
        matches: Option<&mut IndexMap<CaptureKey, Vec<(String, usize)>>>,
        flags: i64,
        offset: usize,
    ) -> anyhow::Result<usize> {
        Self::check_set_order(flags);

        let mut internal: IndexMap<CaptureKey, Vec<(Option<String>, i64)>> = IndexMap::new();
        let result = preg_match_all_offset_capture(
            pattern,
            subject,
            Some(&mut internal),
            flags | PREG_UNMATCHED_AS_NULL | PREG_OFFSET_CAPTURE,
            offset,
        )
        .unwrap_or_else(|| invalid_regex());

        if let Some(out) = matches {
            *out = null_to_empty_offset_match_all(internal);
        }

        Ok(result as usize)
    }

    pub fn replace(pattern: &str, replacement: &str, subject: &str) -> anyhow::Result<String> {
        Self::replace_impl(pattern, replacement, subject, -1, None)
    }

    pub fn replace4(
        pattern: &str,
        replacement: &str,
        subject: &str,
        limit: i64,
    ) -> anyhow::Result<String> {
        Self::replace_impl(pattern, replacement, subject, limit, None)
    }

    pub fn replace5(
        pattern: &str,
        replacement: &str,
        subject: &str,
        limit: i64,
        count: &mut usize,
    ) -> anyhow::Result<String> {
        Self::replace_impl(pattern, replacement, subject, limit, Some(count))
    }

    fn replace_impl(
        pattern: &str,
        replacement: &str,
        subject: &str,
        limit: i64,
        count: Option<&mut usize>,
    ) -> anyhow::Result<String> {
        // `$subject` is statically a string here, so the is_scalar/is_array
        // guards (ARRAY_MSG / INVALID_TYPE_MSG) of the PHP original are
        // unreachable and not reproduced.
        Ok(preg_replace(pattern, replacement, subject, limit, count)
            .unwrap_or_else(|| invalid_regex()))
    }

    pub fn replace_callback<F: FnMut(&IndexMap<CaptureKey, String>) -> String>(
        pattern: &str,
        replacement: F,
        subject: &str,
    ) -> anyhow::Result<String> {
        Self::replace_callback6(pattern, replacement, subject, -1, None, 0)
    }

    pub fn replace_callback6<F: FnMut(&IndexMap<CaptureKey, String>) -> String>(
        pattern: &str,
        mut replacement: F,
        subject: &str,
        limit: i64,
        count: Option<&mut usize>,
        flags: i64,
    ) -> anyhow::Result<String> {
        let adapter = |internal: &IndexMap<CaptureKey, Option<String>>| -> String {
            replacement(&drop_null_matches_ref(internal))
        };

        Ok(
            preg_replace_callback(pattern, adapter, subject, limit, count, flags)
                .unwrap_or_else(|| invalid_regex()),
        )
    }

    pub fn split(pattern: &str, subject: &str) -> anyhow::Result<Vec<String>> {
        Self::split4(pattern, subject, -1, 0)
    }

    pub fn split4(
        pattern: &str,
        subject: &str,
        limit: i64,
        flags: i64,
    ) -> anyhow::Result<Vec<String>> {
        assert!(
            flags & PREG_SPLIT_OFFSET_CAPTURE == 0,
            "PREG_SPLIT_OFFSET_CAPTURE is not supported as it changes the type of $matches, use splitWithOffsets() instead"
        );

        Ok(preg_split(pattern, subject, limit, flags).unwrap_or_else(|| invalid_regex()))
    }

    pub fn grep(pattern: &str, array: &[&str]) -> anyhow::Result<Vec<String>> {
        Self::grep3(pattern, array, 0)
    }

    pub fn grep3(pattern: &str, array: &[&str], flags: i64) -> anyhow::Result<Vec<String>> {
        Ok(preg_grep(pattern, array, flags).unwrap_or_else(|| invalid_regex()))
    }

    pub fn is_match(pattern: &str, subject: &str) -> anyhow::Result<bool> {
        Self::match5(pattern, subject, None, 0, 0)
    }

    pub fn is_match3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut IndexMap<CaptureKey, String>>,
    ) -> anyhow::Result<bool> {
        Self::match5(pattern, subject, matches, 0, 0)
    }

    pub fn is_match5(
        pattern: &str,
        subject: &str,
        matches: Option<&mut IndexMap<CaptureKey, String>>,
        flags: i64,
        offset: usize,
    ) -> anyhow::Result<bool> {
        Self::match5(pattern, subject, matches, flags, offset)
    }

    pub fn is_match_named(
        pattern: &str,
        subject: &str,
        matches: &mut IndexMap<String, String>,
    ) -> anyhow::Result<bool> {
        let mut internal: IndexMap<CaptureKey, Option<String>> = IndexMap::new();
        let result = preg_match(
            pattern,
            subject,
            Some(&mut internal),
            PREG_UNMATCHED_AS_NULL,
            0,
        )
        .unwrap_or_else(|| invalid_regex());

        matches.clear();
        for (key, value) in internal {
            if let (CaptureKey::ByName(name), Some(value)) = (key, value) {
                matches.insert(name, value);
            }
        }

        Ok(result == 1)
    }

    pub fn is_match_with_indexed_captures(
        pattern: &str,
        subject: &str,
    ) -> anyhow::Result<Option<Vec<String>>> {
        // Classic preg_match semantics (no PREG_UNMATCHED_AS_NULL): trailing
        // unmatched groups are truncated, interior unmatched groups become "".
        let mut internal: IndexMap<CaptureKey, Option<String>> = IndexMap::new();
        let result = preg_match(pattern, subject, Some(&mut internal), 0, 0)
            .unwrap_or_else(|| invalid_regex());

        if result == 0 {
            return Ok(None);
        }

        let max_index = internal
            .keys()
            .filter_map(|key| match key {
                CaptureKey::ByIndex(index) => Some(*index),
                CaptureKey::ByName(_) => None,
            })
            .max()
            .unwrap_or(0);

        let mut captures = Vec::with_capacity(max_index + 1);
        for index in 0..=max_index {
            let value = internal
                .get(&CaptureKey::ByIndex(index))
                .and_then(|value| value.clone())
                .unwrap_or_default();
            captures.push(value);
        }

        Ok(Some(captures))
    }

    pub fn is_match_all3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut IndexMap<CaptureKey, Vec<String>>>,
    ) -> anyhow::Result<bool> {
        Ok(Self::match_all5(pattern, subject, matches, 0, 0)? > 0)
    }

    pub fn is_match_all_with_offsets3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut IndexMap<CaptureKey, Vec<(String, usize)>>>,
    ) -> anyhow::Result<bool> {
        Ok(Self::match_all_with_offsets5(pattern, subject, matches, 0, 0)? > 0)
    }

    fn check_offset_capture(flags: i64, use_function_name: &str) {
        assert!(
            flags & PREG_OFFSET_CAPTURE == 0,
            "PREG_OFFSET_CAPTURE is not supported as it changes the type of $matches, use {}() instead",
            use_function_name
        );
    }

    fn check_set_order(flags: i64) {
        assert!(
            flags & PREG_SET_ORDER == 0,
            "PREG_SET_ORDER is not supported as it changes the type of $matches"
        );
    }
}

// Drops `null` (unmatched) groups, mirroring how the public `string`-valued
// `matches` map represents PHP's `string|null` entries by their absence.
fn drop_null_matches(
    matches: IndexMap<CaptureKey, Option<String>>,
) -> IndexMap<CaptureKey, String> {
    matches
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key, value)))
        .collect()
}

fn drop_null_matches_ref(
    matches: &IndexMap<CaptureKey, Option<String>>,
) -> IndexMap<CaptureKey, String> {
    matches
        .iter()
        .filter_map(|(key, value)| value.clone().map(|value| (key.clone(), value)))
        .collect()
}

// In the `Vec<String>`-valued maps a per-iteration `null` cannot be stored, so
// unmatched groups collapse to "" (the classic non-PREG_UNMATCHED_AS_NULL form).
fn null_to_empty_match_all(
    matches: IndexMap<CaptureKey, Vec<Option<String>>>,
) -> IndexMap<CaptureKey, Vec<String>> {
    matches
        .into_iter()
        .map(|(key, values)| {
            (
                key,
                values
                    .into_iter()
                    .map(|value| value.unwrap_or_default())
                    .collect(),
            )
        })
        .collect()
}

fn null_to_empty_offset_match_all(
    matches: IndexMap<CaptureKey, Vec<(Option<String>, i64)>>,
) -> IndexMap<CaptureKey, Vec<(String, usize)>> {
    matches
        .into_iter()
        .map(|(key, values)| {
            (
                key,
                values
                    .into_iter()
                    .map(|(value, offset)| (value.unwrap_or_default(), offset.max(0) as usize))
                    .collect(),
            )
        })
        .collect()
}

#[derive(Debug, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub enum CaptureKey {
    ByIndex(usize),
    ByName(String),
}

// Returns Some(0|1) on success or None when the underlying preg_match returned
// false. Unmatched groups are reported as None (PREG_UNMATCHED_AS_NULL).
pub fn preg_match(
    pattern: &str,
    subject: &str,
    matches: Option<&mut IndexMap<CaptureKey, Option<String>>>,
    flags: i64,
    offset: usize,
) -> Option<i64> {
    let re = shirabe_php_shim::compile_php_pattern(pattern).ok()?;
    let unmatched_as_null = flags & PREG_UNMATCHED_AS_NULL != 0;
    let caps = re.captures_at(subject, offset);

    if let Some(out) = matches {
        out.clear();
        if let Some(caps) = &caps {
            let names: Vec<Option<&str>> = re.capture_names().collect();
            *out = single_match_map(caps, &names, unmatched_as_null);
        }
    }

    Some(if caps.is_some() { 1 } else { 0 })
}

pub fn preg_match_all(
    pattern: &str,
    subject: &str,
    matches: Option<&mut IndexMap<CaptureKey, Vec<Option<String>>>>,
    flags: i64,
    offset: usize,
) -> Option<i64> {
    let re = shirabe_php_shim::compile_php_pattern(pattern).ok()?;
    let unmatched_as_null = flags & PREG_UNMATCHED_AS_NULL != 0;
    let group_count = re.captures_len();
    let names: Vec<Option<&str>> = re.capture_names().collect();

    // PREG_PATTERN_ORDER: one column per group, one row per match occurrence.
    let mut groups: Vec<Vec<Option<String>>> = vec![Vec::new(); group_count];
    let mut count = 0i64;
    for caps in re.captures_iter(&subject[offset..]) {
        count += 1;
        for (g, column) in groups.iter_mut().enumerate() {
            let value = caps.get(g).map(|m| m.as_str().to_string());
            column.push(if unmatched_as_null {
                value
            } else {
                Some(value.unwrap_or_default())
            });
        }
    }

    if let Some(out) = matches {
        out.clear();
        for (g, column) in groups.into_iter().enumerate() {
            if let Some(Some(name)) = names.get(g) {
                out.insert(CaptureKey::ByName((*name).to_string()), column.clone());
            }
            out.insert(CaptureKey::ByIndex(g), column);
        }
    }

    Some(count)
}

pub fn preg_match_all_offset_capture(
    pattern: &str,
    subject: &str,
    matches: Option<&mut IndexMap<CaptureKey, Vec<(Option<String>, i64)>>>,
    flags: i64,
    offset: usize,
) -> Option<i64> {
    let re = shirabe_php_shim::compile_php_pattern(pattern).ok()?;
    let unmatched_as_null = flags & PREG_UNMATCHED_AS_NULL != 0;
    let group_count = re.captures_len();
    let names: Vec<Option<&str>> = re.capture_names().collect();

    let mut groups: Vec<Vec<(Option<String>, i64)>> = vec![Vec::new(); group_count];
    let mut count = 0i64;
    for caps in re.captures_iter(&subject[offset..]) {
        count += 1;
        for (g, column) in groups.iter_mut().enumerate() {
            let entry = match caps.get(g) {
                Some(m) => (Some(m.as_str().to_string()), (m.start() + offset) as i64),
                None if unmatched_as_null => (None, -1),
                None => (Some(String::new()), -1),
            };
            column.push(entry);
        }
    }

    if let Some(out) = matches {
        out.clear();
        for (g, column) in groups.into_iter().enumerate() {
            if let Some(Some(name)) = names.get(g) {
                out.insert(CaptureKey::ByName((*name).to_string()), column.clone());
            }
            out.insert(CaptureKey::ByIndex(g), column);
        }
    }

    Some(count)
}

pub fn preg_replace(
    pattern: &str,
    replacement: &str,
    subject: &str,
    limit: i64,
    count: Option<&mut usize>,
) -> Option<String> {
    let re = shirabe_php_shim::compile_php_pattern(pattern).ok()?;
    let limit = if limit < 0 {
        usize::MAX
    } else {
        limit as usize
    };

    let mut out: Vec<u8> = Vec::new();
    let mut last = 0usize;
    let mut n = 0usize;
    for caps in re.captures_iter(subject) {
        if n >= limit {
            break;
        }
        let m = caps.get(0).unwrap();
        out.extend_from_slice(&subject.as_bytes()[last..m.start()]);
        expand_php_replacement(replacement, &caps, &mut out);
        last = m.end();
        n += 1;
    }
    out.extend_from_slice(&subject.as_bytes()[last..]);

    if let Some(count) = count {
        *count = n;
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

pub fn preg_replace_callback<F: FnMut(&IndexMap<CaptureKey, Option<String>>) -> String>(
    pattern: &str,
    mut callback: F,
    subject: &str,
    limit: i64,
    count: Option<&mut usize>,
    flags: i64,
) -> Option<String> {
    let re = shirabe_php_shim::compile_php_pattern(pattern).ok()?;
    let unmatched_as_null = flags & PREG_UNMATCHED_AS_NULL != 0;
    let names: Vec<Option<&str>> = re.capture_names().collect();
    let limit = if limit < 0 {
        usize::MAX
    } else {
        limit as usize
    };

    let mut out: Vec<u8> = Vec::new();
    let mut last = 0usize;
    let mut n = 0usize;
    for caps in re.captures_iter(subject) {
        if n >= limit {
            break;
        }
        let m = caps.get(0).unwrap();
        out.extend_from_slice(&subject.as_bytes()[last..m.start()]);
        let map = single_match_map(&caps, &names, unmatched_as_null);
        out.extend_from_slice(callback(&map).as_bytes());
        last = m.end();
        n += 1;
    }
    out.extend_from_slice(&subject.as_bytes()[last..]);

    if let Some(count) = count {
        *count = n;
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

pub fn preg_split(pattern: &str, subject: &str, limit: i64, flags: i64) -> Option<Vec<String>> {
    let re = shirabe_php_shim::compile_php_pattern(pattern).ok()?;
    let no_empty = flags & PREG_SPLIT_NO_EMPTY != 0;
    let delim_capture = flags & PREG_SPLIT_DELIM_CAPTURE != 0;
    // `limit` counts the resulting pieces; a non-positive value means no limit.
    let max_delims = if limit > 0 {
        (limit as usize).saturating_sub(1)
    } else {
        usize::MAX
    };

    let mut result: Vec<String> = Vec::new();
    let mut push = |s: &str, result: &mut Vec<String>| {
        if !(no_empty && s.is_empty()) {
            result.push(s.to_string());
        }
    };

    let mut last = 0usize;
    let mut delims = 0usize;
    for caps in re.captures_iter(subject) {
        if delims >= max_delims {
            break;
        }
        let m = caps.get(0).unwrap();
        push(&subject[last..m.start()], &mut result);
        if delim_capture {
            // Mirror preg_match: trailing unmatched groups are dropped, interior
            // unmatched groups are emitted as "".
            if let Some(last_g) = (1..caps.len()).rev().find(|&g| caps.get(g).is_some()) {
                for g in 1..=last_g {
                    push(caps.get(g).map(|x| x.as_str()).unwrap_or(""), &mut result);
                }
            }
        }
        last = m.end();
        delims += 1;
    }
    push(&subject[last..], &mut result);

    Some(result)
}

pub fn preg_grep(pattern: &str, array: &[&str], flags: i64) -> Option<Vec<String>> {
    let re = shirabe_php_shim::compile_php_pattern(pattern).ok()?;
    let invert = flags & PREG_GREP_INVERT != 0;
    Some(
        array
            .iter()
            .filter(|s| re.is_match(s) != invert)
            .map(|s| s.to_string())
            .collect(),
    )
}

// Builds a single match's `$matches` map with both named and numbered keys
// (the named key precedes its number). With PREG_UNMATCHED_AS_NULL, every group
// is present and non-participating ones are None; otherwise classic semantics
// apply: trailing unmatched groups are dropped and interior ones become "".
fn single_match_map(
    caps: &regex::Captures,
    names: &[Option<&str>],
    unmatched_as_null: bool,
) -> IndexMap<CaptureKey, Option<String>> {
    let mut out = IndexMap::new();
    let group_count = caps.len();
    let last_participating = (0..group_count).rev().find(|&i| caps.get(i).is_some());

    for i in 0..group_count {
        let m = caps.get(i);
        if !unmatched_as_null && m.is_none() {
            if let Some(last) = last_participating {
                if i > last {
                    break;
                }
            }
        }
        let value = if unmatched_as_null {
            m.map(|m| m.as_str().to_string())
        } else {
            Some(m.map(|m| m.as_str().to_string()).unwrap_or_default())
        };
        if let Some(Some(name)) = names.get(i) {
            out.insert(CaptureKey::ByName((*name).to_string()), value.clone());
        }
        out.insert(CaptureKey::ByIndex(i), value);
    }
    out
}

// Expands a PHP preg replacement template against `caps`, appending bytes to
// `out`. Backreferences are written as `$1`, `${1}`, `\1` or `\\1`; a literal
// `$` or `\` not forming a reference is emitted verbatim. Out-of-range or
// non-participating groups expand to nothing.
fn expand_php_replacement(template: &str, caps: &regex::Captures, out: &mut Vec<u8>) {
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() => {
                let (group, consumed) = replacement_group(&bytes[i + 1..]);
                if let Some(m) = caps.get(group) {
                    out.extend_from_slice(m.as_str().as_bytes());
                }
                i += 1 + consumed;
            }
            b'\\' if i + 1 < bytes.len() && bytes[i + 1] == b'\\' => {
                out.push(b'\\');
                i += 2;
            }
            b'$' if i + 1 < bytes.len() && bytes[i + 1] == b'{' => {
                let rest = &bytes[i + 2..];
                match rest.iter().position(|&b| b == b'}') {
                    Some(c) if c > 0 && rest[..c].iter().all(|b| b.is_ascii_digit()) => {
                        let group: usize =
                            std::str::from_utf8(&rest[..c]).unwrap().parse().unwrap();
                        if let Some(m) = caps.get(group) {
                            out.extend_from_slice(m.as_str().as_bytes());
                        }
                        i += 2 + c + 1;
                    }
                    _ => {
                        out.push(b'$');
                        i += 1;
                    }
                }
            }
            b'$' if i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() => {
                let (group, consumed) = replacement_group(&bytes[i + 1..]);
                if let Some(m) = caps.get(group) {
                    out.extend_from_slice(m.as_str().as_bytes());
                }
                i += 1 + consumed;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
}

// Reads up to two leading ASCII digits as a PHP backreference group number.
fn replacement_group(bytes: &[u8]) -> (usize, usize) {
    let mut group = 0usize;
    let mut consumed = 0usize;
    while consumed < 2 && consumed < bytes.len() && bytes[consumed].is_ascii_digit() {
        group = group * 10 + (bytes[consumed] - b'0') as usize;
        consumed += 1;
    }
    (group, consumed)
}

/// Panics if a pattern is invalid instead of throwing a PcreException.
/// TODO: takes regex::Error and shows its message
fn invalid_regex() -> ! {
    panic!("invalid regex");
}
