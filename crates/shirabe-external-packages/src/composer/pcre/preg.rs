#[derive(Debug)]
pub struct Preg;

impl Preg {
    pub fn r#match(pattern: &str, subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match4(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
        flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match5(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
        flags: i64,
        offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_strict_groups(pattern: &str, subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_strict_groups3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_strict_groups4(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
        flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_strict_groups5(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
        flags: i64,
        offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_with_offsets(pattern: &str, subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_with_offsets3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, (String, usize)>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_with_offsets4(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, (String, usize)>>,
        flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_with_offsets5(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, (String, usize)>>,
        flags: i64,
        offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_all(pattern: &str, subject: &str) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all4(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
        flags: i64,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all5(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
        flags: i64,
        offset: usize,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all_strict_groups(pattern: &str, subject: &str) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all_strict_groups3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all_strict_groups4(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
        flags: i64,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all_strict_groups5(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
        flags: i64,
        offset: usize,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all_with_offsets(pattern: &str, subject: &str) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all_with_offsets3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<(String, usize)>>>,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all_with_offsets4(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<(String, usize)>>>,
        flags: i64,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all_with_offsets5(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<(String, usize)>>>,
        flags: i64,
        offset: usize,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn replace(pattern: &str, replacement: &str, subject: &str) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace4(
        pattern: &str,
        replacement: &str,
        subject: &str,
        limit: i64,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace5(
        pattern: &str,
        replacement: &str,
        subject: &str,
        limit: i64,
        count: &mut usize,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback<F: Fn(&indexmap::IndexMap<CaptureKey, String>) -> String>(
        pattern: &str,
        replacement: F,
        subject: &str,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback4<F: Fn(&indexmap::IndexMap<CaptureKey, String>) -> String>(
        pattern: &str,
        replacement: F,
        subject: &str,
        limit: i64,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback5<F: Fn(&indexmap::IndexMap<CaptureKey, String>) -> String>(
        pattern: &str,
        replacement: F,
        subject: &str,
        limit: i64,
        count: &mut usize,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback6<F: Fn(&indexmap::IndexMap<CaptureKey, String>) -> String>(
        pattern: &str,
        replacement: F,
        subject: &str,
        limit: i64,
        count: Option<&mut usize>,
        flags: i64,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback_array(
        pattern: &indexmap::IndexMap<String, String>,
        subject: &str,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback_array3(
        pattern: &indexmap::IndexMap<String, String>,
        subject: &str,
        limit: i64,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback_array4(
        pattern: &indexmap::IndexMap<String, String>,
        subject: &str,
        limit: i64,
        count: &mut usize,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback_array5(
        pattern: &indexmap::IndexMap<String, String>,
        subject: &str,
        limit: i64,
        count: Option<&mut usize>,
        flags: i64,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn split(pattern: &str, subject: &str) -> anyhow::Result<Vec<String>> {
        todo!()
    }

    pub fn split3(pattern: &str, subject: &str, limit: i64) -> anyhow::Result<Vec<String>> {
        todo!()
    }

    pub fn split4(
        pattern: &str,
        subject: &str,
        limit: i64,
        flags: i64,
    ) -> anyhow::Result<Vec<String>> {
        todo!()
    }

    pub fn split_with_offsets(
        pattern: &str,
        subject: &str,
    ) -> anyhow::Result<Vec<(String, usize)>> {
        todo!()
    }

    pub fn split_with_offsets3(
        pattern: &str,
        subject: &str,
        limit: i64,
    ) -> anyhow::Result<Vec<(String, usize)>> {
        todo!()
    }

    pub fn split_with_offsets4(
        pattern: &str,
        subject: &str,
        limit: i64,
        flags: i64,
    ) -> anyhow::Result<Vec<(String, usize)>> {
        todo!()
    }

    pub fn grep(pattern: &str, array: &[&str]) -> anyhow::Result<Vec<String>> {
        todo!()
    }

    pub fn grep3(pattern: &str, array: &[&str], flags: i64) -> anyhow::Result<Vec<String>> {
        todo!()
    }

    pub fn is_match(pattern: &str, subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match4(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
        flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match5(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
        flags: i64,
        offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_strict_groups(pattern: &str, subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_strict_groups3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_strict_groups4(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
        flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_strict_groups5(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
        flags: i64,
        offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_with_offsets(pattern: &str, subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_with_offsets3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, (String, usize)>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_with_offsets4(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, (String, usize)>>,
        flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_with_offsets5(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, (String, usize)>>,
        flags: i64,
        offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all(pattern: &str, subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all4(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
        flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all5(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
        flags: i64,
        offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all_strict_groups(pattern: &str, subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all_strict_groups3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all_strict_groups4(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
        flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all_strict_groups5(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
        flags: i64,
        offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all_with_offsets(pattern: &str, subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all_with_offsets3(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<(String, usize)>>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all_with_offsets4(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<(String, usize)>>>,
        flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all_with_offsets5(
        pattern: &str,
        subject: &str,
        matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<(String, usize)>>>,
        flags: i64,
        offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }
}

#[derive(Debug, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub enum CaptureKey {
    ByIndex(usize),
    ByName(String),
}
