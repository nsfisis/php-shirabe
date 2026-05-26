//! ref: composer/src/Composer/Downloader/DownloaderInterface.php

use crate::package::PackageInterfaceHandle;
use shirabe_php_shim::PhpMixed;

#[async_trait::async_trait(?Send)]
pub trait DownloaderInterface: std::fmt::Debug {
    fn get_installation_source(&self) -> String;

    async fn download(
        &self,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>>;

    /// Convenience for the PHP default `$output = true` overload.
    async fn download3(
        &self,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.download(package, path, prev_package, true).await
    }

    async fn prepare(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn install(
        &self,
        package: PackageInterfaceHandle,
        path: &str,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>>;

    /// Convenience for the PHP default `$output = true` overload.
    async fn install2(
        &self,
        package: PackageInterfaceHandle,
        path: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.install(package, path, true).await
    }

    async fn update(
        &self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn remove(
        &self,
        package: PackageInterfaceHandle,
        path: &str,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>>;

    /// Convenience for the PHP default `$output = true` overload.
    async fn remove2(
        &self,
        package: PackageInterfaceHandle,
        path: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.remove(package, path, true).await
    }

    async fn cleanup(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>>;

    /// TODO(phase-b): runtime downcast helpers for PHP `instanceof` checks.
    fn as_change_report_interface(&self) -> Option<&dyn crate::downloader::ChangeReportInterface> {
        None
    }

    /// TODO(phase-b): runtime downcast helpers for PHP `instanceof` checks.
    fn as_vcs_capable_downloader_interface(
        &self,
    ) -> Option<&dyn crate::downloader::VcsCapableDownloaderInterface> {
        None
    }

    /// TODO(phase-b): runtime downcast helpers for PHP `instanceof` checks.
    fn as_dvcs_downloader_interface(
        &self,
    ) -> Option<&dyn crate::downloader::DvcsDownloaderInterface> {
        None
    }
}
