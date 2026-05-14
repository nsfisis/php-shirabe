//! ref: composer/src/Composer/Downloader/DownloaderInterface.php

use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;

use crate::package::package_interface::PackageInterface;

pub trait DownloaderInterface {
    fn get_installation_source(&self) -> String;

    fn download(&self, package: &dyn PackageInterface, path: &str, prev_package: Option<&dyn PackageInterface>) -> anyhow::Result<Box<dyn PromiseInterface>>;

    fn prepare(&self, r#type: &str, package: &dyn PackageInterface, path: &str, prev_package: Option<&dyn PackageInterface>) -> anyhow::Result<Box<dyn PromiseInterface>>;

    fn install(&self, package: &dyn PackageInterface, path: &str) -> anyhow::Result<Box<dyn PromiseInterface>>;

    fn update(&self, initial: &dyn PackageInterface, target: &dyn PackageInterface, path: &str) -> anyhow::Result<Box<dyn PromiseInterface>>;

    fn remove(&self, package: &dyn PackageInterface, path: &str) -> anyhow::Result<Box<dyn PromiseInterface>>;

    fn cleanup(&self, r#type: &str, package: &dyn PackageInterface, path: &str, prev_package: Option<&dyn PackageInterface>) -> anyhow::Result<Box<dyn PromiseInterface>>;
}
