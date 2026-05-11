//! ref: composer/src/Composer/Downloader/VcsCapableDownloaderInterface.php

use crate::package::package_interface::PackageInterface;

pub trait VcsCapableDownloaderInterface {
    fn get_vcs_reference(&self, package: &dyn PackageInterface, path: String) -> Option<String>;
}
