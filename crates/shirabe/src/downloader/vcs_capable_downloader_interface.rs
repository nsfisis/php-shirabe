//! ref: composer/src/Composer/Downloader/VcsCapableDownloaderInterface.php

use crate::package::PackageInterfaceHandle;

pub trait VcsCapableDownloaderInterface {
    fn get_vcs_reference(&self, package: PackageInterfaceHandle, path: String) -> Option<String>;
}
