//! ref: composer/src/Composer/Downloader/ChangeReportInterface.php

use crate::package::PackageInterfaceHandle;
use anyhow::Result;

pub trait ChangeReportInterface {
    fn get_local_changes(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
    ) -> Result<Option<String>>;
}
