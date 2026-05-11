//! ref: composer/src/Composer/Downloader/DvcsDownloaderInterface.php

use crate::package::package_interface::PackageInterface;

pub trait DvcsDownloaderInterface {
    fn get_unpushed_changes(&self, package: &dyn PackageInterface, path: String) -> Option<String>;
}
