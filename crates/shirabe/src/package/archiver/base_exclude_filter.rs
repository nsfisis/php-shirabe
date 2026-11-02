//! ref: composer/src/Composer/Package/Archiver/BaseExcludeFilter.php

use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{bconcat, ltrim2, preg_is_match, trim, trim2};
use shirabe_symfony_finder::Glob;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct BaseExcludeFilterBase {
    pub source_path: PathBuf,
    pub exclude_patterns: Vec<(String, bool, bool)>,
}

impl BaseExcludeFilterBase {
    pub fn new(source_path: PathBuf) -> Self {
        Self {
            source_path,
            exclude_patterns: Vec::new(),
        }
    }

    pub fn parse_lines<F>(&self, lines: Vec<BString>, line_parser: F) -> Vec<(String, bool, bool)>
    where
        F: Fn(&BStr) -> Option<(String, bool, bool)>,
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

    pub fn generate_patterns(&self, rules: Vec<BString>) -> Vec<(String, bool, bool)> {
        rules
            .into_iter()
            .map(|rule| Self::generate_pattern(rule.as_bstr()))
            .collect()
    }

    pub fn generate_pattern(rule: &BStr) -> (String, bool, bool) {
        let mut negate = false;
        let mut pattern = String::new();

        let mut rule = BString::from(rule);
        if rule.starts_with(b"!") {
            negate = true;
            rule = ltrim2(&rule, "!");
        }

        let first_slash_position = rule.find_byte(b'/');
        if first_slash_position == Some(0) {
            pattern = "^/".to_string();
        } else if first_slash_position.is_none() || first_slash_position == Some(rule.len() - 1) {
            pattern = "/".to_string();
        }

        let rule = trim2(&rule, "/");

        // Regex pattern compatibility:
        // PHP strips the delimiters/anchors off Glob::toRegex output and appends the
        // `(?=$|/)` look-ahead, which the regex crate cannot compile. The boundary has
        // to participate in Glob's no-dot union expansion, so it is woven into the
        // body by to_regex_dir_boundary instead of being appended here.
        let rule_regex = Glob::to_regex_dir_boundary(&rule, true, true);

        (
            bconcat!("{", pattern, rule_regex, "}")
                .display_lossy()
                .to_string(),
            negate,
            false,
        )
    }
}

pub trait BaseExcludeFilter {
    fn source_path(&self) -> &Path;
    fn exclude_patterns(&self) -> &[(String, bool, bool)];
    fn exclude_patterns_mut(&mut self) -> &mut Vec<(String, bool, bool)>;

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
