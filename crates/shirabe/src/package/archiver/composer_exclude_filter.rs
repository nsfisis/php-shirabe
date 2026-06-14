//! ref: composer/src/Composer/Package/Archiver/ComposerExcludeFilter.php

use super::BaseExcludeFilter;
use super::BaseExcludeFilterBase;

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

impl BaseExcludeFilter for ComposerExcludeFilter {
    fn source_path(&self) -> &str {
        &self.inner.source_path
    }

    fn exclude_patterns(&self) -> &[(String, bool, bool)] {
        &self.inner.exclude_patterns
    }

    fn exclude_patterns_mut(&mut self) -> &mut Vec<(String, bool, bool)> {
        &mut self.inner.exclude_patterns
    }
}
