//! ref: composer/src/Composer/Package/Archiver/ArchiverInterface.php

pub trait ArchiverInterface {
    fn archive(
        &self,
        sources: String,
        target: String,
        format: String,
        excludes: Vec<String>,
        ignore_filters: bool,
    ) -> String;

    fn supports(&self, format: String, source_type: Option<String>) -> bool;
}
