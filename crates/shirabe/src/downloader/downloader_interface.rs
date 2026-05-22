//! ref: composer/src/Composer/Downloader/DownloaderInterface.php

use crate::package::PackageInterface;
use shirabe_php_shim::PhpMixed;

pub trait DownloaderInterface: std::fmt::Debug {
    fn get_installation_source(&self) -> String;

    async fn download(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>>;

    /// Convenience for the PHP default `$output = true` overload.
    async fn download3(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.download(package, path, prev_package, true)
    }

    async fn prepare(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn install(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>>;

    /// Convenience for the PHP default `$output = true` overload.
    async fn install2(
        &self,
        package: &dyn PackageInterface,
        path: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.install(package, path, true)
    }

    async fn update(
        &self,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
        path: &str,
    ) -> anyhow::Result<Option<PhpMixed>>;

    async fn remove(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>>;

    /// Convenience for the PHP default `$output = true` overload.
    async fn remove2(
        &self,
        package: &dyn PackageInterface,
        path: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.remove(package, path, true)
    }

    async fn cleanup(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
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
