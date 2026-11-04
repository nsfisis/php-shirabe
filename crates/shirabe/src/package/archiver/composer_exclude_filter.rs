//! ref: composer/src/Composer/Package/Archiver/ComposerExcludeFilter.php sha1:43fdb55ca6e70b15f416c66fc0ca8db8e893c8d8

use super::{BaseExcludeFilter, BaseExcludeFilterBase};
use bstr::BString;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ComposerExcludeFilter {
    inner: BaseExcludeFilterBase,
}

impl ComposerExcludeFilter {
    pub fn new(source_path: impl Into<PathBuf>, exclude_rules: Vec<BString>) -> Self {
        let mut inner = BaseExcludeFilterBase::new(source_path.into());
        inner.exclude_patterns = inner.generate_patterns(exclude_rules);
        Self { inner }
    }
}

impl BaseExcludeFilter for ComposerExcludeFilter {
    fn source_path(&self) -> &Path {
        &self.inner.source_path
    }

    fn exclude_patterns(&self) -> &[(BString, bool, bool)] {
        &self.inner.exclude_patterns
    }

    fn exclude_patterns_mut(&mut self) -> &mut Vec<(BString, bool, bool)> {
        &mut self.inner.exclude_patterns
    }
}
