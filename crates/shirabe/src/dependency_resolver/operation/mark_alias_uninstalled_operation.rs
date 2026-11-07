//! ref: composer/src/Composer/DependencyResolver/Operation/MarkAliasUninstalledOperation.php sha1:54996c301f9f9ccf77d193bb4b79764dca745aba

use crate::dependency_resolver::operation::SolverOperation;
use crate::package::{AliasPackageHandle, DisplayMode};
use bstr::BStr;
use shirabe_php_shim::{bconcat, php_str};

#[derive(Debug, Clone)]
pub struct MarkAliasUninstalledOperation {
    package: AliasPackageHandle,
}

impl MarkAliasUninstalledOperation {
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
                .get_full_pretty_version(true, DisplayMode::SourceRefIfDev),
            "</comment>) as uninstalled, alias of <info>",
            self.package.get_alias_of().get_pretty_name(),
            "</info> (<comment>",
            self.package
                .get_alias_of()
                .get_full_pretty_version(true, DisplayMode::SourceRefIfDev),
            "</comment>)"
        )
    }
}

impl SolverOperation for MarkAliasUninstalledOperation {
    const TYPE: &'static BStr = php_str(b"markAliasUninstalled");
}

impl std::fmt::Display for MarkAliasUninstalledOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.show(false).display_lossy())
    }
}
