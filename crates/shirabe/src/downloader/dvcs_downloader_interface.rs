//! ref: composer/src/Composer/Downloader/DvcsDownloaderInterface.php

use crate::package::PackageInterfaceHandle;
use anyhow::Result;

pub trait DvcsDownloaderInterface {
    fn get_unpushed_changes(
        &self,
        package: PackageInterfaceHandle,
        path: String,
    ) -> Result<Option<String>>;
}
