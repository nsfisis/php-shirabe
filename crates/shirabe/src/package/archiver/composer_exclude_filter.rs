//! ref: composer/src/Composer/Package/Archiver/ComposerExcludeFilter.php

use super::base_exclude_filter::BaseExcludeFilter;

#[derive(Debug)]
pub struct ComposerExcludeFilter {
    inner: BaseExcludeFilter,
}

impl ComposerExcludeFilter {
    pub fn new(source_path: String, exclude_rules: Vec<String>) -> Self {
        let mut inner = BaseExcludeFilter::new(source_path);
        inner.exclude_patterns = inner.generate_patterns(exclude_rules);
        Self { inner }
    }
}
