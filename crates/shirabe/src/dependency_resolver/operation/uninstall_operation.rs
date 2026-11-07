//! ref: composer/src/Composer/DependencyResolver/Operation/UninstallOperation.php sha1:19d2137e34a47ec48e5332263060009eba3e894e

use crate::dependency_resolver::operation::SolverOperation;
use crate::package::{DisplayMode, PackageInterfaceHandle};
use bstr::BStr;
use shirabe_php_shim::{bconcat, php_str};

#[derive(Debug, Clone)]
pub struct UninstallOperation {
    package: PackageInterfaceHandle,
}

impl UninstallOperation {
    pub fn new(package: PackageInterfaceHandle) -> Self {
        Self { package }
    }

    pub fn get_package(&self) -> PackageInterfaceHandle {
        self.package.clone()
    }

    pub fn show(&self, lock: bool) -> bstr::BString {
        Self::format(self.package.clone(), lock)
    }

    pub fn format(package: PackageInterfaceHandle, _lock: bool) -> bstr::BString {
        bconcat!(
            "Removing <info>",
            package.get_pretty_name(),
            "</info> (<comment>",
            package.get_full_pretty_version(true, DisplayMode::SourceRefIfDev),
            "</comment>)"
        )
    }
}

impl SolverOperation for UninstallOperation {
    const TYPE: &'static BStr = php_str(b"uninstall");
}

impl std::fmt::Display for UninstallOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.show(false).display_lossy())
    }
}
