//! ref: composer/src/Composer/DependencyResolver/Operation/InstallOperation.php sha1:d31107e28df4ef1adfa315b011fe7826d98c722a

use crate::dependency_resolver::operation::SolverOperation;
use crate::package::{DisplayMode, PackageInterfaceHandle};
use bstr::{BStr, BString};
use shirabe_php_shim::{PhpDisplay, bformat, php_str};

#[derive(Debug, Clone)]
pub struct InstallOperation {
    package: PackageInterfaceHandle,
}

impl InstallOperation {
    pub fn new(package: PackageInterfaceHandle) -> Self {
        Self { package }
    }

    pub fn get_package(&self) -> PackageInterfaceHandle {
        self.package.clone()
    }

    pub fn show(&self, lock: bool) -> bstr::BString {
        Self::format(self.package.clone(), lock)
    }

    pub fn format(package: PackageInterfaceHandle, lock: bool) -> bstr::BString {
        bformat!(
            "{}<info>{}</info> (<comment>{}</comment>)",
            if lock { "Locking " } else { "Installing " },
            package.get_pretty_name(),
            package.get_full_pretty_version(true, DisplayMode::SourceRefIfDev)
        )
    }
}

impl SolverOperation for InstallOperation {
    const TYPE: &'static BStr = php_str(b"install");
}

impl PhpDisplay for InstallOperation {
    fn fmt(&self, out: &mut BString) {
        out.extend_from_slice(&self.show(false));
    }
}
