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
    /// The named capture groups of a single match, keyed by group name alone.
    pub struct PregNamedGroups(String => String);
}

#[derive(Debug)]
pub struct Preg;

impl Preg {
    pub fn match3<'h>(pattern: impl PregPattern, subject: &'h str) -> Option<PregMatches<'h>> {
        Self::match4(pattern, subject, 0)
    }

    pub fn match4<'h>(
        pattern: impl PregPattern,
        subject: &'h str,
        offset: usize,
    ) -> Option<PregMatches<'h>> {
        preg_match2(pattern, subject, offset)
    }

    pub fn match_all(pattern: impl PregPattern, subject: &str) -> usize {
        Self::match_all2(pattern, subject).occurrence_count()
    }

    pub fn match_all2(pattern: impl PregPattern, subject: &str) -> PregMatchesAll {
        preg_match_all2(pattern, subject)
    }

    fn match_all_with_offsets5(
        pattern: impl PregPattern,
        subject: &str,
    ) -> PregMatchesAllWithOffsets {
        preg_match_all_offset_capture(pattern, subject)
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

    pub fn replace_callback<'h, F: FnMut(&PregMatches<'h>) -> String>(
        pattern: impl PregPattern,
        mut replacement: F,
        subject: &'h str,
    ) -> String {
        let adapter = |matches: &PregMatches<'h>| Ok(replacement(matches));

        preg_replace_callback(pattern, adapter, subject).expect("$replacement cannot fail")
    }

    pub fn grep<T: AsRef<str>>(
        pattern: impl PregPattern,
        array: impl IntoIterator<Item = T>,
    ) -> impl Iterator<Item = T> {
        preg_grep(pattern, array)
    }

    pub fn is_match(pattern: impl PregPattern, subject: &str) -> bool {
        Self::match4(pattern, subject, 0).is_some()
    }

    pub fn is_match3<'h>(pattern: impl PregPattern, subject: &'h str) -> Option<PregMatches<'h>> {
        Self::match4(pattern, subject, 0)
    }

    pub fn is_match4<'h>(
        pattern: impl PregPattern,
        subject: &'h str,
        offset: usize,
    ) -> Option<PregMatches<'h>> {
        Self::match4(pattern, subject, offset)
    }

    pub fn is_match_named(pattern: impl PregPattern, subject: &str) -> Option<PregNamedGroups> {
        Some(
            preg_match2(pattern, subject, 0)?
                .iter()
                .filter_map(|(key, value)| match (key, value) {
                    (CaptureKey::ByName(name), Some(value)) => Some((name, value.to_string())),
                    _ => None,
                })
                .collect(),
        )
    }

    /// `is_match3` with the groups positioned by number rather than keyed, for callers that only
    /// read numbered groups. Index 0 is the full match; an unmatched group is `None`.
    pub fn is_match_with_indexed_captures(
        pattern: impl PregPattern,
        subject: &str,
    ) -> Option<Vec<Option<String>>> {
        Some(
            preg_match2(pattern, subject, 0)?
                .iter()
                .filter_map(|(key, value)| match key {
                    CaptureKey::ByIndex(_) => Some(value.map(str::to_string)),
                    CaptureKey::ByName(_) => None,
                })
                .collect(),
        )
    }

    pub fn is_match_all(pattern: impl PregPattern, subject: &str) -> PregMatchesAll {
        Self::match_all2(pattern, subject)
    }

    pub fn is_match_all_with_offsets3(
        pattern: impl PregPattern,
        subject: &str,
    ) -> PregMatchesAllWithOffsets {
        Self::match_all_with_offsets5(pattern, subject)
    }
}
