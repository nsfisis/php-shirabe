//! ref: composer/src/Composer/DependencyResolver/Operation/MarkAliasInstalledOperation.php sha1:52a7354ef1ccc5439bbe76480a311fc4c365b1d7

use crate::dependency_resolver::operation::SolverOperation;
use crate::package::{AliasPackageHandle, DisplayMode};
use bstr::{BStr, BString};
use shirabe_php_shim::{PhpDisplay, bformat, php_str};

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
        bformat!(
            "Marking <info>{}</info> (<comment>{}</comment>) as installed, alias of <info>{}</info> (<comment>{}</comment>)",
            self.package.get_pretty_name(),
            self.package
                .get_full_pretty_version(true, DisplayMode::SourceRefIfDev),
            self.package.get_alias_of().get_pretty_name(),
            self.package
                .get_alias_of()
                .get_full_pretty_version(true, DisplayMode::SourceRefIfDev)
        )
    }
}

impl SolverOperation for MarkAliasInstalledOperation {
    const TYPE: &'static BStr = php_str(b"markAliasInstalled");
}

impl PhpDisplay for MarkAliasInstalledOperation {
    fn fmt(&self, out: &mut BString) {
        out.extend_from_slice(&self.show(false));
    }
}
