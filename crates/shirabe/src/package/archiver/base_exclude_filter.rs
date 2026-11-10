//! ref: composer/src/Composer/Package/Archiver/BaseExcludeFilter.php sha1:16a9cbcc09a710ac828d7dd91d2b1812411fdb9d

use bstr::{BStr, BString, ByteSlice as _};
use shirabe_php_shim::{bformat, ltrim_chars, preg_is_match, trim, trim_chars};
use shirabe_symfony_finder::Glob;

#[derive(Debug)]
pub struct BaseExcludeFilterBase {
    pub source_path: std::path::PathBuf,
    pub exclude_patterns: Vec<(BString, bool, bool)>,
}

impl BaseExcludeFilterBase {
    pub fn new(source_path: std::path::PathBuf) -> Self {
        Self {
            source_path,
            exclude_patterns: Vec::new(),
        }
    }

    pub fn parse_lines<F>(&self, lines: Vec<BString>, line_parser: F) -> Vec<(BString, bool, bool)>
    where
        F: Fn(&BStr) -> Option<(BString, bool, bool)>,
    {
        lines
            .into_iter()
            .filter_map(|line| {
                let line = trim(&line);
                if line.is_empty() || line.starts_with(b"#") {
                    return None;
                }
                line_parser(line.as_bstr())
            })
            .collect()
    }

    pub fn generate_patterns(&self, rules: Vec<BString>) -> Vec<(BString, bool, bool)> {
        rules
            .into_iter()
            .map(|rule| Self::generate_pattern(rule.as_bstr()))
            .collect()
    }

    pub fn generate_pattern(rule: &BStr) -> (BString, bool, bool) {
        let mut negate = false;
        let mut pattern = BString::default();

        let mut rule = BString::from(rule);
        if rule.starts_with(b"!") {
            negate = true;
            rule = ltrim_chars(&rule, "!");
        }

        let first_slash_position = rule.find_byte(b'/');
        if first_slash_position == Some(0) {
            pattern = BString::from("^/");
        } else if first_slash_position.is_none() || first_slash_position == Some(rule.len() - 1) {
            pattern = BString::from("/");
        }

        let rule = trim_chars(&rule, "/");

        // Regex pattern compatibility:
        // PHP strips the delimiters/anchors off Glob::toRegex output and appends the
        // `(?=$|/)` look-ahead, which the regex crate cannot compile. The boundary has
        // to participate in Glob's no-dot union expansion, so it is woven into the
        // body by to_regex_dir_boundary instead of being appended here.
        let rule_regex = Glob::to_regex_dir_boundary(&rule, true, true);

        (bformat!("{{{pattern}{rule_regex}}}"), negate, false)
    }
}

pub trait BaseExcludeFilter {
    fn source_path(&self) -> &std::path::Path;
    fn exclude_patterns(&self) -> &[(BString, bool, bool)];
    fn exclude_patterns_mut(&mut self) -> &mut Vec<(BString, bool, bool)>;

    /// Checks the given path against all exclude patterns in this filter
    ///
    /// Negated patterns overwrite exclude decisions of previous filters.
    fn filter(&self, relative_path: &BStr, mut exclude: bool) -> bool {
        for (pattern, negate, strip_leading_slash) in self.exclude_patterns() {
            let path = if *strip_leading_slash {
                &relative_path[1..]
            } else {
                relative_path
            };

            if preg_is_match(pattern, path) {
                exclude = !negate;
            }
        }

        exclude
    }
}
