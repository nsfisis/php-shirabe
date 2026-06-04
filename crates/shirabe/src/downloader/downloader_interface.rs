//! ref: composer/src/Composer/Downloader/DownloaderInterface.php

use crate::package::PackageInterfaceHandle;
use shirabe_php_shim::PhpMixed;

#[async_trait::async_trait(?Send)]
pub trait DownloaderInterface: std::fmt::Debug {
    fn get_installation_source(&self) -> String;

    async fn download(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>>;

    /// Convenience for the PHP default `$output = true` overload.
    async fn download3(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.download(package, path, prev_package, true).await
    }

    async fn prepare(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn install(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>>;

    /// Convenience for the PHP default `$output = true` overload.
    async fn install2(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.install(package, path, true).await
    }

    async fn update(
        &mut self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn remove(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>>;

    /// Convenience for the PHP default `$output = true` overload.
    async fn remove2(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.remove(package, path, true).await
    }

    async fn cleanup(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>>;

    fn as_change_report_interface(
        &mut self,
    ) -> Option<&mut dyn crate::downloader::ChangeReportInterface> {
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
