//! ref: composer/src/Composer/Downloader/DvcsDownloaderInterface.php

use crate::package::PackageInterfaceHandle;

pub trait DvcsDownloaderInterface {
    fn get_unpushed_changes(
        &self,
        package: PackageInterfaceHandle,
        path: String,
    ) -> anyhow::Result<Option<String>>;
}
