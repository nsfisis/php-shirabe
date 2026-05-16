//! ref: composer/src/Composer/Package/Archiver/BaseExcludeFilter.php

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::component::finder::glob::Glob;

#[derive(Debug)]
pub struct BaseExcludeFilter {
    pub(crate) source_path: String,
    pub(crate) exclude_patterns: Vec<(String, bool, bool)>,
}

impl BaseExcludeFilter {
    pub fn new(source_path: String) -> Self {
        Self {
            source_path,
            exclude_patterns: vec![],
        }
    }

    pub fn filter(&self, relative_path: &str, mut exclude: bool) -> bool {
        for (pattern, negate, strip_leading_slash) in &self.exclude_patterns {
            let path = if *strip_leading_slash {
                &relative_path[1..]
            } else {
                relative_path
            };

            // suppressed RuntimeException, equivalent to PHP try-catch
            if let Ok(matched) = Preg::is_match(pattern, path) {
                if matched {
                    exclude = !negate;
                }
            }
        }

        exclude
    }

    pub fn parse_lines<F>(&self, lines: Vec<String>, line_parser: F) -> Vec<(String, bool, bool)>
    where
        F: Fn(&str) -> Option<(String, bool, bool)>,
    {
        lines
            .into_iter()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    return None;
                }
                line_parser(line)
            })
            .collect()
    }

    pub fn generate_patterns(&self, rules: Vec<String>) -> Vec<(String, bool, bool)> {
        rules
            .into_iter()
            .map(|rule| self.generate_pattern(&rule))
            .collect()
    }

    pub fn generate_pattern(&self, rule: &str) -> (String, bool, bool) {
        let mut negate = false;
        let mut pattern = String::new();

        let mut rule = rule.to_string();
        if !rule.is_empty() && rule.starts_with('!') {
            negate = true;
            rule = rule.trim_start_matches('!').to_string();
        }

        let first_slash_position = rule.find('/');
        if first_slash_position == Some(0) {
            pattern = "^/".to_string();
        } else if first_slash_position.is_none() || first_slash_position == Some(rule.len() - 1) {
            pattern = "/".to_string();
        }

        let rule = rule.trim_matches('/');

        // remove delimiters as well as caret (^) and dollar sign ($) from the regex
        let glob_regex = Glob::to_regex(rule);
        let rule_regex = &glob_regex[2..glob_regex.len() - 2];

        (format!("{}{}(?=$|/)", pattern, rule_regex), negate, false)
    }
}
