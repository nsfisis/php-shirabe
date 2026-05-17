//! ref: composer/src/Composer/Package/Archiver/GitExcludeFilter.php

use crate::package::archiver::base_exclude_filter::BaseExcludeFilterBase;
use shirabe_external_packages::composer::pcre::preg::Preg;
use std::path::Path;

pub struct GitExcludeFilter {
    inner: BaseExcludeFilterBase,
}

impl GitExcludeFilter {
    pub fn new(source_path: String) -> Self {
        let inner = BaseExcludeFilterBase::new(source_path.clone());
        let mut filter = Self { inner };

        let gitattributes_path = format!("{}/.gitattributes", source_path);
        if Path::new(&gitattributes_path).exists() {
            let lines: Vec<String> = std::fs::read_to_string(&gitattributes_path)
                .unwrap_or_default()
                .lines()
                .map(|l| l.to_string())
                .collect();
            let patterns = filter.inner.parse_lines(lines, |line| {
                GitExcludeFilter::parse_git_attributes_line_static(line)
            });
            filter.inner.exclude_patterns.extend(patterns);
        }

        filter
    }

    pub fn parse_git_attributes_line(&self, line: &str) -> Option<(String, bool, bool)> {
        Self::parse_git_attributes_line_static(line)
    }

    fn parse_git_attributes_line_static(line: &str) -> Option<(String, bool, bool)> {
        let parts = Preg::split(r"\s+", line);

        if parts.len() == 2 && parts[1] == "export-ignore" {
            return Some(BaseExcludeFilterBase::generate_pattern(&parts[0]));
        }

        if parts.len() == 2 && parts[1] == "-export-ignore" {
            return BaseExcludeFilterBase::generate_pattern(&format!("!{}", parts[0]));
        }

        None
    }
}
