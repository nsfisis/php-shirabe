//! ref: composer/src/Composer/DependencyResolver/Operation/UninstallOperation.php sha1:19d2137e34a47ec48e5332263060009eba3e894e

use crate::dependency_resolver::operation::SolverOperation;
use crate::package::{DisplayMode, PackageInterfaceHandle};
use bstr::{BStr, BString};
use shirabe_php_shim::{PhpDisplay, bformat, php_str};

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
        bformat!(
            "Removing <info>{}</info> (<comment>{}</comment>)",
            package.get_pretty_name(),
            package.get_full_pretty_version(true, DisplayMode::SourceRefIfDev)
        )
    }
}

impl SolverOperation for UninstallOperation {
    const TYPE: &'static BStr = php_str(b"uninstall");
}

impl PhpDisplay for UninstallOperation {
    fn fmt(&self, out: &mut BString) {
        out.extend_from_slice(&self.show(false));
    }
}
