//! ref: composer/src/Composer/Downloader/DownloaderInterface.php sha1:ff1543a20f529978e3ab0539c09ed7ce90a93f70

use crate::package::PackageInterfaceHandle;
use bstr::{BStr, BString};
use shirabe_php_shim::{PhpClass, PhpMixed};

#[async_trait::async_trait(?Send)]
pub trait DownloaderInterface: std::fmt::Debug + PhpClass {
    fn get_installation_source(&self) -> BString;

    async fn download(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        prev_package: Option<PackageInterfaceHandle>,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>>;

    /// Convenience for the PHP default `$output = true` overload.
    async fn download3(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.download(package, path, prev_package, true).await
    }

    async fn prepare(
        &self,
        r#type: &BStr,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn install(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>>;

    /// Convenience for the PHP default `$output = true` overload.
    async fn install2(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.install(package, path, true).await
    }

    async fn update(
        &self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn remove(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>>;

    /// Convenience for the PHP default `$output = true` overload.
    async fn remove2(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.remove(package, path, true).await
    }

    async fn cleanup(
        &self,
        r#type: &BStr,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>>;

    fn as_change_report_interface(&self) -> Option<&dyn crate::downloader::ChangeReportInterface> {
        None
    }

    fn as_vcs_capable_downloader_interface(
        &self,
    ) -> Option<&dyn crate::downloader::VcsCapableDownloaderInterface> {
        None
    }

    fn as_dvcs_downloader_interface(
        &self,
    ) -> Option<&dyn crate::downloader::DvcsDownloaderInterface> {
        None
    }
}
