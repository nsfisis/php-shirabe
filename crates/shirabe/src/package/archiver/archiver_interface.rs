//! ref: composer/src/Composer/Package/Archiver/ArchiverInterface.php

pub trait ArchiverInterface {
    fn archive(
        &self,
        sources: String,
        target: String,
        format: String,
        excludes: Vec<String>,
        ignore_filters: bool,
    ) -> anyhow::Result<String>;

    fn supports(&self, format: String, source_type: Option<String>) -> bool;

    /// PHP `$archiver instanceof X` checks; allow downcasting from `dyn ArchiverInterface`.
    fn as_any(&self) -> &dyn std::any::Any;
}
