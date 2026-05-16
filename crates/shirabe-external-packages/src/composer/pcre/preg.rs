use indexmap::IndexMap;

#[derive(Debug)]
pub struct Preg;

impl Preg {
    pub fn is_match(pattern: &str, subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn replace(pattern: &str, replacement: &str, subject: &str) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback<F>(pattern: &str, callback: F, subject: &str) -> anyhow::Result<String>
    where
        F: Fn(&IndexMap<String, String>) -> String,
    {
        todo!()
    }

    pub fn replace_with_count(
        pattern: &str,
        replacement: &str,
        subject: &str,
        count: &mut i64,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn split(pattern: &str, subject: &str) -> anyhow::Result<Vec<String>> {
        todo!()
    }

    pub fn grep(pattern: &str, input: Vec<String>) -> anyhow::Result<Vec<String>> {
        todo!()
    }

    /// Returns captures as a flat Vec indexed by group number (index 0 = full match).
    pub fn is_match_strict_groups(pattern: &str, subject: &str) -> Option<Vec<String>> {
        todo!()
    }

    /// Returns named captures in an IndexMap.
    pub fn is_match_named(
        pattern: &str,
        subject: &str,
        matches: &mut IndexMap<String, String>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    /// Returns all matches; outer Vec indexed by group number, inner Vec by match occurrence.
    pub fn is_match_all_strict_groups(pattern: &str, subject: &str) -> Option<Vec<Vec<String>>> {
        todo!()
    }

    /// Returns captures with byte offsets: IndexMap<group_name, Vec<(match_str, offset)>>.
    pub fn is_match_all_with_offsets(
        pattern: &str,
        subject: &str,
        matches: &mut IndexMap<String, Vec<(String, i64)>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    /// Returns indexed captures as Vec (index 0 = full match) when pattern matches.
    pub fn is_match_with_indexed_captures(
        pattern: &str,
        subject: &str,
    ) -> anyhow::Result<Option<Vec<String>>> {
        todo!()
    }

    /// Like is_match_strict_groups but returns named captures as IndexMap.
    pub fn match_strict_groups(pattern: &str, subject: &str) -> Option<IndexMap<String, String>> {
        todo!()
    }
}
