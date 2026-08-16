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
    PregPattern, preg_grep, preg_match_all_offset_capture, preg_match_all2, preg_match_map,
    preg_match2, preg_replace_callback, preg_replace2,
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
        let internal = preg_match2(pattern, subject, offset);

        if let Some(out) = matches {
            *out = match &internal {
                Some(internal) => drop_null_matches(internal),
                None => PregMatchedGroups::new(),
            };
        }

        internal.is_some()
    }

    pub fn match_all(pattern: impl PregPattern, subject: &str) -> usize {
        occurrence_count(&preg_match_all2(pattern, subject))
    }

    pub fn match_all2(
        pattern: impl PregPattern,
        subject: &str,
        matches: &mut PregMatchesAll,
    ) -> usize {
        *matches = preg_match_all2(pattern, subject);
        occurrence_count(matches)
    }

    fn match_all_with_offsets5(
        pattern: impl PregPattern,
        subject: &str,
        matches: Option<&mut PregMatchesAllWithOffsets>,
    ) -> usize {
        let internal = preg_match_all_offset_capture(pattern, subject);
        let count = internal[&CaptureKey::ByIndex(0)].len();

        if let Some(out) = matches {
            *out = internal;
        }

        count
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
        let adapter = |internal: &PregMatches| Ok(replacement(&drop_null_matches(internal)));

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
        let internal = preg_match2(pattern, subject, 0);
        let result = internal.is_some();

        matches.clear();
        if let Some(internal) = internal {
            for (key, value) in internal {
                if let (CaptureKey::ByName(name), Some(value)) = (key, value) {
                    matches.insert(name, value);
                }
            }
        }

        result
    }

    /// `is_match3` with the groups positioned by number rather than keyed, for callers that only
    /// read numbered groups. Index 0 is the full match; an unmatched group is `None`.
    pub fn is_match_with_indexed_captures(
        pattern: impl PregPattern,
        subject: &str,
    ) -> Option<Vec<Option<String>>> {
        Some(
            preg_match2(pattern, subject, 0)?
                .into_iter()
                .filter_map(|(key, value)| match key {
                    CaptureKey::ByIndex(_) => Some(value),
                    CaptureKey::ByName(_) => None,
                })
                .collect(),
        )
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
fn drop_null_matches(matches: &PregMatches) -> PregMatchedGroups {
    matches
        .iter()
        .filter_map(|(key, value)| value.clone().map(|value| (key.clone(), value)))
        .collect()
}

// PHP's `preg_match_all` returns the number of occurrences; every column of a
// PREG_PATTERN_ORDER map holds one entry per occurrence.
fn occurrence_count(matches: &PregMatchesAll) -> usize {
    matches[&CaptureKey::ByIndex(0)].len()
}
