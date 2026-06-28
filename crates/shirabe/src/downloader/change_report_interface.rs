//! ref: composer/src/Composer/Downloader/ChangeReportInterface.php

use crate::package::PackageInterfaceHandle;

pub trait ChangeReportInterface {
    fn get_local_changes(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
    ) -> anyhow::Result<Option<String>>;
}
