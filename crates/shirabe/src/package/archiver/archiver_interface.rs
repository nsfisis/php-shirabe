//! ref: composer/src/Composer/Package/Archiver/ArchiverInterface.php

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
