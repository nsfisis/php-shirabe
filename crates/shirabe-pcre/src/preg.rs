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
//!
//! See docs/dev/regex-porting.md for more detailed regex porting rules.

use indexmap::IndexMap;
pub use shirabe_php_shim::CaptureKey;
use shirabe_php_shim::{
    PREG_UNMATCHED_AS_NULL, PregPattern, preg_grep, preg_match_all_offset_capture, preg_match_all2,
    preg_match2, preg_replace_callback, preg_replace2,
};

#[derive(Debug)]
pub struct Preg;

impl Preg {
    pub fn match3(
        pattern: impl PregPattern,
        subject: &str,
        matches: Option<&mut IndexMap<CaptureKey, String>>,
    ) -> bool {
        Self::match5(pattern, subject, matches, 0, 0)
    }

    pub fn match5(
        pattern: impl PregPattern,
        subject: &str,
        matches: Option<&mut IndexMap<CaptureKey, String>>,
        flags: i64,
        offset: usize,
    ) -> bool {
        let mut internal: IndexMap<CaptureKey, Option<String>> = IndexMap::new();
        let result = preg_match2(
            pattern,
            subject,
            &mut internal,
            flags | PREG_UNMATCHED_AS_NULL,
            offset,
        );

        if let Some(out) = matches {
            *out = drop_null_matches(internal);
        }

        result
    }

    pub fn match_all(pattern: impl PregPattern, subject: &str) -> usize {
        let mut dummy = IndexMap::new();
        preg_match_all2(pattern, subject, &mut dummy)
    }

    pub fn match_all2(
        pattern: impl PregPattern,
        subject: &str,
        matches: &mut IndexMap<CaptureKey, Vec<Option<String>>>,
    ) -> usize {
        preg_match_all2(pattern, subject, matches)
    }

    fn match_all_with_offsets5(
        pattern: impl PregPattern,
        subject: &str,
        matches: Option<&mut IndexMap<CaptureKey, Vec<(Option<String>, i64)>>>,
    ) -> usize {
        let mut internal: IndexMap<CaptureKey, Vec<(Option<String>, i64)>> = IndexMap::new();
        let result =
            preg_match_all_offset_capture(pattern, subject, &mut internal, PREG_UNMATCHED_AS_NULL);

        if let Some(out) = matches {
            *out = internal;
        }

        result
    }

    pub fn replace(pattern: impl PregPattern, replacement: &str, subject: &str) -> String {
        preg_replace2(pattern, replacement, subject, -1, None)
    }

    pub fn replace4(
        pattern: impl PregPattern,
        replacement: &str,
        subject: &str,
        limit: i64,
    ) -> String {
        preg_replace2(pattern, replacement, subject, limit, None)
    }

    pub fn replace5(
        pattern: impl PregPattern,
        replacement: &str,
        subject: &str,
        limit: i64,
        count: &mut usize,
    ) -> String {
        preg_replace2(pattern, replacement, subject, limit, Some(count))
    }

    pub fn replace_callback<F: FnMut(&IndexMap<CaptureKey, String>) -> String>(
        pattern: impl PregPattern,
        mut replacement: F,
        subject: &str,
    ) -> String {
        let adapter = |internal: &IndexMap<CaptureKey, Option<String>>| {
            Ok(replacement(&drop_null_matches_ref(internal)))
        };

        preg_replace_callback(pattern, adapter, subject).expect("$replacement cannot fail")
    }

    pub fn grep<T: AsRef<str>>(
        pattern: impl PregPattern,
        array: impl IntoIterator<Item = T>,
    ) -> impl Iterator<Item = T> {
        preg_grep(pattern, array)
    }

    pub fn is_match(pattern: impl PregPattern, subject: &str) -> bool {
        Self::match5(pattern, subject, None, 0, 0)
    }

    pub fn is_match3(
        pattern: impl PregPattern,
        subject: &str,
        matches: Option<&mut IndexMap<CaptureKey, String>>,
    ) -> bool {
        Self::match5(pattern, subject, matches, 0, 0)
    }

    pub fn is_match5(
        pattern: impl PregPattern,
        subject: &str,
        matches: Option<&mut IndexMap<CaptureKey, String>>,
        flags: i64,
        offset: usize,
    ) -> bool {
        Self::match5(pattern, subject, matches, flags, offset)
    }

    pub fn is_match_named(
        pattern: impl PregPattern,
        subject: &str,
        matches: &mut IndexMap<String, String>,
    ) -> bool {
        let mut internal: IndexMap<CaptureKey, Option<String>> = IndexMap::new();
        let result = preg_match2(pattern, subject, &mut internal, PREG_UNMATCHED_AS_NULL, 0);

        matches.clear();
        for (key, value) in internal {
            if let (CaptureKey::ByName(name), Some(value)) = (key, value) {
                matches.insert(name, value);
            }
        }

        result
    }

    pub fn is_match_with_indexed_captures(
        pattern: impl PregPattern,
        subject: &str,
    ) -> Option<Vec<String>> {
        // Classic preg_match semantics (no PREG_UNMATCHED_AS_NULL): trailing
        // unmatched groups are truncated, interior unmatched groups become "".
        let mut internal: IndexMap<CaptureKey, Option<String>> = IndexMap::new();
        let result = preg_match2(pattern, subject, &mut internal, 0, 0);

        if !result {
            return None;
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

        Some(captures)
    }

    pub fn is_match_all(
        pattern: impl PregPattern,
        subject: &str,
        matches: &mut IndexMap<CaptureKey, Vec<Option<String>>>,
    ) -> bool {
        Self::match_all2(pattern, subject, matches) > 0
    }

    pub fn is_match_all_with_offsets3(
        pattern: impl PregPattern,
        subject: &str,
        matches: Option<&mut IndexMap<CaptureKey, Vec<(Option<String>, i64)>>>,
    ) -> bool {
        Self::match_all_with_offsets5(pattern, subject, matches) > 0
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
