//! ref: composer/src/Composer/Downloader/ChangeReportInterface.php

use anyhow::Result;

use crate::package::package_interface::PackageInterface;

pub trait ChangeReportInterface {
    fn get_local_changes(&self, package: &dyn PackageInterface, path: &str) -> Result<Option<String>>;
}
