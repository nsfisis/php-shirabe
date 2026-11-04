//! ref: composer/src/Composer/Downloader/DvcsDownloaderInterface.php sha1:48c885abec6e9f6ab7c0317a1983f4b216b5d26d

use crate::package::PackageInterfaceHandle;

pub trait DvcsDownloaderInterface {
    fn get_unpushed_changes(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<bstr::BString>>;
}
