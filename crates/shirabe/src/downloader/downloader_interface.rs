//! ref: composer/src/Composer/Downloader/DownloaderInterface.php

use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;

use crate::package::package_interface::PackageInterface;

pub trait DownloaderInterface: std::fmt::Debug {
    fn get_installation_source(&self) -> String;

    fn download(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
        output: bool,
    ) -> anyhow::Result<Box<dyn PromiseInterface>>;

    /// Convenience for the PHP default `$output = true` overload.
    fn download3(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> anyhow::Result<Box<dyn PromiseInterface>> {
        self.download(package, path, prev_package, true)
    }

    fn prepare(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> anyhow::Result<Box<dyn PromiseInterface>>;

    fn install(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        output: bool,
    ) -> anyhow::Result<Box<dyn PromiseInterface>>;

    /// Convenience for the PHP default `$output = true` overload.
    fn install2(
        &self,
        package: &dyn PackageInterface,
        path: &str,
    ) -> anyhow::Result<Box<dyn PromiseInterface>> {
        self.install(package, path, true)
    }

    fn update(
        &self,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
        path: &str,
    ) -> anyhow::Result<Box<dyn PromiseInterface>>;

    fn remove(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        output: bool,
    ) -> anyhow::Result<Box<dyn PromiseInterface>>;

    /// Convenience for the PHP default `$output = true` overload.
    fn remove2(
        &self,
        package: &dyn PackageInterface,
        path: &str,
    ) -> anyhow::Result<Box<dyn PromiseInterface>> {
        self.remove(package, path, true)
    }

    fn cleanup(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> anyhow::Result<Box<dyn PromiseInterface>>;

    /// TODO(phase-b): runtime downcast helpers for PHP `instanceof` checks.
    fn as_change_report_interface(
        &self,
    ) -> Option<&dyn crate::downloader::change_report_interface::ChangeReportInterface> {
        None
    }

    /// TODO(phase-b): runtime downcast helpers for PHP `instanceof` checks.
    fn as_vcs_capable_downloader_interface(
        &self,
    ) -> Option<
        &dyn crate::downloader::vcs_capable_downloader_interface::VcsCapableDownloaderInterface,
    > {
        None
    }

    /// TODO(phase-b): runtime downcast helpers for PHP `instanceof` checks.
    fn as_dvcs_downloader_interface(
        &self,
    ) -> Option<&dyn crate::downloader::dvcs_downloader_interface::DvcsDownloaderInterface> {
        None
    }
}
