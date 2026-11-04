//! ref: composer/src/Composer/Package/Archiver/GitExcludeFilter.php sha1:f49884ce2fd5ab13e64dc4c89f38bafce1946657

use crate::package::archiver::{BaseExcludeFilter, BaseExcludeFilterBase};
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{bconcat, file, file_exists, path_from_bytes, php_regex, preg_split};
use std::path::{Path, PathBuf};

pub struct GitExcludeFilter {
    inner: BaseExcludeFilterBase,
}

impl GitExcludeFilter {
    pub fn new(source_path: impl Into<PathBuf>) -> Self {
        let source_path = source_path.into();
        let gitattributes_path = bconcat!(&source_path, "/.gitattributes");
        let inner = BaseExcludeFilterBase::new(source_path);
        let mut filter = Self { inner };

        let gitattributes_path = path_from_bytes(&gitattributes_path);
        if file_exists(gitattributes_path) {
            let lines = file(gitattributes_path, 0).unwrap_or_default();
            let patterns = filter
                .inner
                .parse_lines(lines, Self::parse_git_attributes_line_static);
            filter.inner.exclude_patterns.extend(patterns);
        }

        filter
    }

    pub fn parse_git_attributes_line(
        &self,
        line: impl AsRef<[u8]>,
    ) -> Option<(BString, bool, bool)> {
        Self::parse_git_attributes_line_static(line.as_ref().as_bstr())
    }

    fn parse_git_attributes_line_static(line: &BStr) -> Option<(BString, bool, bool)> {
        let parts = preg_split(php_regex!(r"#\s+#"), line);

        if parts.len() == 2 && parts[1] == "export-ignore" {
            return Some(BaseExcludeFilterBase::generate_pattern(parts[0].as_bstr()));
        }

        if parts.len() == 2 && parts[1] == "-export-ignore" {
            return Some(BaseExcludeFilterBase::generate_pattern(
                bconcat!("!", &parts[0]).as_bstr(),
            ));
        }

        None
    }
}

impl BaseExcludeFilter for GitExcludeFilter {
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
