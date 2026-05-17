#[derive(Debug)]
pub struct Preg;

impl Preg {
    pub fn r#match(_pattern: &str, _subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match3(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match4(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
        _flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match5(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
        _flags: i64,
        _offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_strict_groups(_pattern: &str, _subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_strict_groups3(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_strict_groups4(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
        _flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_strict_groups5(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
        _flags: i64,
        _offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_with_offsets(_pattern: &str, _subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_with_offsets3(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, (String, usize)>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_with_offsets4(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, (String, usize)>>,
        _flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_with_offsets5(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, (String, usize)>>,
        _flags: i64,
        _offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn match_all(_pattern: &str, _subject: &str) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all3(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all4(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
        _flags: i64,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all5(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
        _flags: i64,
        _offset: usize,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all_strict_groups(_pattern: &str, _subject: &str) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all_strict_groups3(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all_strict_groups4(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
        _flags: i64,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all_strict_groups5(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
        _flags: i64,
        _offset: usize,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all_with_offsets(_pattern: &str, _subject: &str) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all_with_offsets3(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<(String, usize)>>>,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all_with_offsets4(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<(String, usize)>>>,
        _flags: i64,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn match_all_with_offsets5(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<(String, usize)>>>,
        _flags: i64,
        _offset: usize,
    ) -> anyhow::Result<usize> {
        todo!()
    }

    pub fn replace(_pattern: &str, _replacement: &str, _subject: &str) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace4(
        _pattern: &str,
        _replacement: &str,
        _subject: &str,
        _limit: i64,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace5(
        _pattern: &str,
        _replacement: &str,
        _subject: &str,
        _limit: i64,
        _count: &mut usize,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback<F: Fn(&indexmap::IndexMap<CaptureKey, String>) -> String>(
        _pattern: &str,
        _replacement: F,
        _subject: &str,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback4<F: Fn(&indexmap::IndexMap<CaptureKey, String>) -> String>(
        _pattern: &str,
        _replacement: F,
        _subject: &str,
        _limit: i64,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback5<F: Fn(&indexmap::IndexMap<CaptureKey, String>) -> String>(
        _pattern: &str,
        _replacement: F,
        _subject: &str,
        _limit: i64,
        _count: &mut usize,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback6<F: Fn(&indexmap::IndexMap<CaptureKey, String>) -> String>(
        _pattern: &str,
        _replacement: F,
        _subject: &str,
        _limit: i64,
        _count: Option<&mut usize>,
        _flags: i64,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback_array(
        _pattern: &indexmap::IndexMap<String, String>,
        _subject: &str,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback_array3(
        _pattern: &indexmap::IndexMap<String, String>,
        _subject: &str,
        _limit: i64,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback_array4(
        _pattern: &indexmap::IndexMap<String, String>,
        _subject: &str,
        _limit: i64,
        _count: &mut usize,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn replace_callback_array5(
        _pattern: &indexmap::IndexMap<String, String>,
        _subject: &str,
        _limit: i64,
        _count: Option<&mut usize>,
        _flags: i64,
    ) -> anyhow::Result<String> {
        todo!()
    }

    pub fn split(_pattern: &str, _subject: &str) -> anyhow::Result<Vec<String>> {
        todo!()
    }

    pub fn split3(_pattern: &str, _subject: &str, _limit: i64) -> anyhow::Result<Vec<String>> {
        todo!()
    }

    pub fn split4(
        _pattern: &str,
        _subject: &str,
        _limit: i64,
        _flags: i64,
    ) -> anyhow::Result<Vec<String>> {
        todo!()
    }

    pub fn split_with_offsets(
        _pattern: &str,
        _subject: &str,
    ) -> anyhow::Result<Vec<(String, usize)>> {
        todo!()
    }

    pub fn split_with_offsets3(
        _pattern: &str,
        _subject: &str,
        _limit: i64,
    ) -> anyhow::Result<Vec<(String, usize)>> {
        todo!()
    }

    pub fn split_with_offsets4(
        _pattern: &str,
        _subject: &str,
        _limit: i64,
        _flags: i64,
    ) -> anyhow::Result<Vec<(String, usize)>> {
        todo!()
    }

    pub fn grep(_pattern: &str, _array: &[&str]) -> anyhow::Result<Vec<String>> {
        todo!()
    }

    pub fn grep3(_pattern: &str, _array: &[&str], _flags: i64) -> anyhow::Result<Vec<String>> {
        todo!()
    }

    pub fn is_match(_pattern: &str, _subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match3(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match4(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
        _flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match5(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
        _flags: i64,
        _offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_strict_groups(_pattern: &str, _subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_strict_groups3(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_strict_groups4(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
        _flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_strict_groups5(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, String>>,
        _flags: i64,
        _offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_with_offsets(_pattern: &str, _subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_with_offsets3(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, (String, usize)>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_with_offsets4(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, (String, usize)>>,
        _flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_with_offsets5(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, (String, usize)>>,
        _flags: i64,
        _offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all(_pattern: &str, _subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all3(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all4(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
        _flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all5(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
        _flags: i64,
        _offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all_strict_groups(_pattern: &str, _subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all_strict_groups3(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all_strict_groups4(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
        _flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all_strict_groups5(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<String>>>,
        _flags: i64,
        _offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all_with_offsets(_pattern: &str, _subject: &str) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all_with_offsets3(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<(String, usize)>>>,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all_with_offsets4(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<(String, usize)>>>,
        _flags: i64,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn is_match_all_with_offsets5(
        _pattern: &str,
        _subject: &str,
        _matches: Option<&mut indexmap::IndexMap<CaptureKey, Vec<(String, usize)>>>,
        _flags: i64,
        _offset: usize,
    ) -> anyhow::Result<bool> {
        todo!()
    }
}

#[derive(Debug, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub enum CaptureKey {
    ByIndex(usize),
    ByName(String),
}
