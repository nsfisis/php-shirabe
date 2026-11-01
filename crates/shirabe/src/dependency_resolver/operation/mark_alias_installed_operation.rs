//! ref: composer/src/Composer/DependencyResolver/Operation/MarkAliasInstalledOperation.php

use crate::dependency_resolver::operation::SolverOperation;
use crate::package::AliasPackageHandle;
use shirabe_php_shim::bconcat;

#[derive(Debug, Clone)]
pub struct MarkAliasInstalledOperation {
    package: AliasPackageHandle,
}

impl MarkAliasInstalledOperation {
    pub fn new(package: AliasPackageHandle) -> Self {
        Self { package }
    }

    pub fn get_package(&self) -> AliasPackageHandle {
        self.package.clone()
    }

    pub fn show(&self, _lock: bool) -> bstr::BString {
        bconcat!(
            "Marking <info>",
            self.package.get_pretty_name(),
            "</info> (<comment>",
            self.package
                .get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev),
            "</comment>) as installed, alias of <info>",
            self.package.get_alias_of().get_pretty_name(),
            "</info> (<comment>",
            self.package
                .get_alias_of()
                .get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev),
            "</comment>)"
        )
    }
}

impl SolverOperation for MarkAliasInstalledOperation {
    const TYPE: &'static str = "markAliasInstalled";
}

impl std::fmt::Display for MarkAliasInstalledOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.show(false).display_lossy())
    }
}
