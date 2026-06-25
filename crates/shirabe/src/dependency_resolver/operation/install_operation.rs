//! ref: composer/src/Composer/DependencyResolver/Operation/InstallOperation.php

use crate::dependency_resolver::operation::OperationInterface;
use crate::dependency_resolver::operation::SolverOperation;
use crate::package::PackageInterfaceHandle;

#[derive(Debug)]
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

impl OperationInterface for InstallOperation {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_operation_type(&self) -> String {
        Self::TYPE.to_string()
    }

    fn show(&self, lock: bool) -> String {
        Self::format(self.package.clone(), lock)
    }

    fn as_install_operation(&self) -> Option<&InstallOperation> {
        Some(self)
    }

    fn get_package(&self) -> PackageInterfaceHandle {
        self.package.clone()
    }
}

impl std::fmt::Display for InstallOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.show(false))
    }
}
