//! ref: composer/src/Composer/DependencyResolver/Operation/MarkAliasInstalledOperation.php sha1:52a7354ef1ccc5439bbe76480a311fc4c365b1d7

use crate::dependency_resolver::operation::SolverOperation;
use crate::package::{AliasPackageHandle, DisplayMode};
use bstr::{BStr, BString};
use shirabe_php_shim::{Stringable, bconcat, php_str};

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
                .get_full_pretty_version(true, DisplayMode::SourceRefIfDev),
            "</comment>) as installed, alias of <info>",
            self.package.get_alias_of().get_pretty_name(),
            "</info> (<comment>",
            self.package
                .get_alias_of()
                .get_full_pretty_version(true, DisplayMode::SourceRefIfDev),
            "</comment>)"
        )
    }
}

impl SolverOperation for MarkAliasInstalledOperation {
    const TYPE: &'static BStr = php_str(b"markAliasInstalled");
}

impl Stringable for MarkAliasInstalledOperation {
    fn to_php_string(&self) -> BString {
        self.show(false)
    }
}
