//! ref: composer/src/Composer/Package/Archiver/ArchiverInterface.php sha1:db7d9e46c3bfbf8e0e4a596450f08057c2381d73

use bstr::{BStr, BString};
use std::path::{Path, PathBuf};

pub trait ArchiverInterface {
    fn archive(
        &self,
        sources: &Path,
        target: &Path,
        format: &BStr,
        excludes: Vec<BString>,
        ignore_filters: bool,
    ) -> anyhow::Result<PathBuf>;

    fn supports(&self, format: &BStr, source_type: Option<&BStr>) -> bool;

    /// PHP `$archiver instanceof X` checks; allow downcasting from `dyn ArchiverInterface`.
    fn as_any(&self) -> &dyn std::any::Any;
}
