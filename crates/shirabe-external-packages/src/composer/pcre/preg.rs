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

pub fn preg_last_error() -> i64 {
    todo!()
}

pub fn preg_last_error_msg() -> String {
    todo!()
}

// Returns Some(0|1) on success or None when the underlying preg_match returned
// false. Unmatched groups are reported as None (PREG_UNMATCHED_AS_NULL).
pub fn preg_match(
    _pattern: &str,
    _subject: &str,
    _matches: Option<&mut IndexMap<CaptureKey, Option<String>>>,
    _flags: i64,
    _offset: usize,
) -> Option<i64> {
    todo!()
}

pub fn preg_match_all(
    _pattern: &str,
    _subject: &str,
    _matches: Option<&mut IndexMap<CaptureKey, Vec<Option<String>>>>,
    _flags: i64,
    _offset: usize,
) -> Option<i64> {
    todo!()
}

pub fn preg_match_all_offset_capture(
    _pattern: &str,
    _subject: &str,
    _matches: Option<&mut IndexMap<CaptureKey, Vec<(Option<String>, i64)>>>,
    _flags: i64,
    _offset: usize,
) -> Option<i64> {
    todo!()
}

pub fn preg_replace(
    _pattern: &str,
    _replacement: &str,
    _subject: &str,
    _limit: i64,
    _count: Option<&mut usize>,
) -> Option<String> {
    todo!()
}

pub fn preg_replace_callback<F: FnMut(&IndexMap<CaptureKey, Option<String>>) -> String>(
    _pattern: &str,
    _callback: F,
    _subject: &str,
    _limit: i64,
    _count: Option<&mut usize>,
    _flags: i64,
) -> Option<String> {
    todo!()
}

pub fn preg_split(_pattern: &str, _subject: &str, _limit: i64, _flags: i64) -> Option<Vec<String>> {
    todo!()
}

pub fn preg_grep(_pattern: &str, _array: &[&str], _flags: i64) -> Option<Vec<String>> {
    todo!()
}

/// Panics if a pattern is invalid instead of throwing a PcreException.
/// TODO: takes regex::Error and shows its message
fn invalid_regex() -> ! {
    panic!("invalid regex");
}
