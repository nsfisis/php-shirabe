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

pub use shirabe_php_shim::{CaptureKey, PregMatches, PregMatchesAll, PregMatchesAllWithOffsets};
use shirabe_php_shim::{
    PregPattern, preg_grep, preg_match_all_offset_capture_unmatched_as_null, preg_match_all2,
    preg_match_map, preg_match2, preg_match2_unmatched_as_null, preg_replace_callback,
    preg_replace2,
};

preg_match_map! {
    /// A single match's `$matches` as `Preg` hands it to callers: an unmatched capture group is
    /// absent rather than held as a null value.
    pub struct PregMatchedGroups(CaptureKey => String);
}

preg_match_map! {
    /// The named capture groups of a single match, keyed by group name alone.
    pub struct PregNamedGroups(String => String);
}

#[derive(Debug)]
pub struct Preg;

impl Preg {
    pub fn match3(
        pattern: impl PregPattern,
        subject: &str,
        matches: Option<&mut PregMatchedGroups>,
    ) -> bool {
        Self::match4(pattern, subject, matches, 0)
    }

    pub fn match4(
        pattern: impl PregPattern,
        subject: &str,
        matches: Option<&mut PregMatchedGroups>,
        offset: usize,
    ) -> bool {
        let mut internal = PregMatches::new();
        let result = preg_match2_unmatched_as_null(pattern, subject, &mut internal, offset);

        if let Some(out) = matches {
            *out = drop_null_matches(internal);
        }

        result
    }

    pub fn match_all(pattern: impl PregPattern, subject: &str) -> usize {
        let mut dummy = PregMatchesAll::new();
        preg_match_all2(pattern, subject, &mut dummy)
    }

    pub fn match_all2(
        pattern: impl PregPattern,
        subject: &str,
        matches: &mut PregMatchesAll,
    ) -> usize {
        preg_match_all2(pattern, subject, matches)
    }

    fn match_all_with_offsets5(
        pattern: impl PregPattern,
        subject: &str,
        matches: Option<&mut PregMatchesAllWithOffsets>,
    ) -> usize {
        let mut internal = PregMatchesAllWithOffsets::new();
        let result =
            preg_match_all_offset_capture_unmatched_as_null(pattern, subject, &mut internal);

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

    pub fn replace_callback<F: FnMut(&PregMatchedGroups) -> String>(
        pattern: impl PregPattern,
        mut replacement: F,
        subject: &str,
    ) -> String {
        let adapter = |internal: &PregMatches| Ok(replacement(&drop_null_matches_ref(internal)));

        preg_replace_callback(pattern, adapter, subject).expect("$replacement cannot fail")
    }

    pub fn grep<T: AsRef<str>>(
        pattern: impl PregPattern,
        array: impl IntoIterator<Item = T>,
    ) -> impl Iterator<Item = T> {
        preg_grep(pattern, array)
    }

    pub fn is_match(pattern: impl PregPattern, subject: &str) -> bool {
        Self::match4(pattern, subject, None, 0)
    }

    pub fn is_match3(
        pattern: impl PregPattern,
        subject: &str,
        matches: Option<&mut PregMatchedGroups>,
    ) -> bool {
        Self::match4(pattern, subject, matches, 0)
    }

    pub fn is_match4(
        pattern: impl PregPattern,
        subject: &str,
        matches: Option<&mut PregMatchedGroups>,
        offset: usize,
    ) -> bool {
        Self::match4(pattern, subject, matches, offset)
    }

    pub fn is_match_named(
        pattern: impl PregPattern,
        subject: &str,
        matches: &mut PregNamedGroups,
    ) -> bool {
        let mut internal = PregMatches::new();
        let result = preg_match2_unmatched_as_null(pattern, subject, &mut internal, 0);

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
        let mut internal = PregMatches::new();
        let result = preg_match2(pattern, subject, &mut internal, 0);

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
        matches: &mut PregMatchesAll,
    ) -> bool {
        Self::match_all2(pattern, subject, matches) > 0
    }

    pub fn is_match_all_with_offsets3(
        pattern: impl PregPattern,
        subject: &str,
        matches: Option<&mut PregMatchesAllWithOffsets>,
    ) -> bool {
        Self::match_all_with_offsets5(pattern, subject, matches) > 0
    }
}

// Drops `null` (unmatched) groups, mirroring how the public `string`-valued
// `matches` map represents PHP's `string|null` entries by their absence.
fn drop_null_matches(matches: PregMatches) -> PregMatchedGroups {
    matches
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key, value)))
        .collect()
}

fn drop_null_matches_ref(matches: &PregMatches) -> PregMatchedGroups {
    matches
        .iter()
        .filter_map(|(key, value)| value.clone().map(|value| (key.clone(), value)))
        .collect()
}
