//! ref: composer/src/Composer/DependencyResolver/Operation/InstallOperation.php

use crate::dependency_resolver::operation::SolverOperation;
use crate::package::PackageInterfaceHandle;

#[derive(Debug, Clone)]
pub struct InstallOperation {
    pub(crate) package: PackageInterfaceHandle,
}

impl InstallOperation {
    pub fn new(package: PackageInterfaceHandle) -> Self {
        Self { package }
    }

    pub fn get_package(&self) -> PackageInterfaceHandle {
        self.package.clone()
    }

    pub fn show(&self, lock: bool) -> String {
        Self::format(self.package.clone(), lock)
    }

    pub fn format(package: PackageInterfaceHandle, lock: bool) -> String {
        format!(
            "{}<info>{}</info> (<comment>{}</comment>)",
            if lock { "Locking " } else { "Installing " },
            package.get_pretty_name(),
            package.get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev),
        )
    }
}

impl SolverOperation for InstallOperation {
    const TYPE: &'static str = "install";
}

impl std::fmt::Display for InstallOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.show(false))
    }
}
