//! ref: composer/src/Composer/Downloader/ChangeReportInterface.php

use anyhow::Result;

use crate::package::PackageInterfaceHandle;

pub trait ChangeReportInterface {
    fn get_local_changes(
        &self,
        package: PackageInterfaceHandle,
        path: &str,
    ) -> Result<Option<String>>;
}
