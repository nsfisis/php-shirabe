//! ref: composer/src/Composer/Package/Archiver/ComposerExcludeFilter.php

use super::base_exclude_filter::BaseExcludeFilterBase;

#[derive(Debug)]
pub struct ComposerExcludeFilter {
    inner: BaseExcludeFilterBase,
}

impl ComposerExcludeFilter {
    pub fn new(source_path: String, exclude_rules: Vec<String>) -> Self {
        let mut inner = BaseExcludeFilterBase::new(source_path);
        inner.exclude_patterns = inner.generate_patterns(exclude_rules);
        Self { inner }
    }
}
