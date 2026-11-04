//! ref: composer/src/Composer/Downloader/VcsCapableDownloaderInterface.php sha1:bd36e4e76818618ff9d2bf3ba7b4ffcbc9fcda6b

use crate::package::PackageInterfaceHandle;

pub trait VcsCapableDownloaderInterface {
    fn get_vcs_reference(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<bstr::BString>>;
}
