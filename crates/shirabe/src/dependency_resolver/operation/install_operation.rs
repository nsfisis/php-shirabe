//! ref: composer/src/Composer/DependencyResolver/Operation/InstallOperation.php

use crate::dependency_resolver::operation::SolverOperation;
use crate::package::PackageInterfaceHandle;
use shirabe_php_shim::bconcat;

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
        bconcat!(
            if lock { "Locking " } else { "Installing " },
            "<info>",
            package.get_pretty_name(),
            "</info> (<comment>",
            package.get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev),
            "</comment>)"
        )
    }
}

impl SolverOperation for InstallOperation {
    const TYPE: &'static str = "install";
}

impl std::fmt::Display for InstallOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.show(false).display_lossy())
    }
}
