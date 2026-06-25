//! ref: composer/src/Composer/DependencyResolver/Operation/UninstallOperation.php

use crate::dependency_resolver::operation::OperationInterface;
use crate::dependency_resolver::operation::SolverOperation;
use crate::package::PackageInterfaceHandle;

#[derive(Debug)]
pub struct UninstallOperation {
    pub(crate) package: PackageInterfaceHandle,
}

impl UninstallOperation {
    pub fn new(package: PackageInterfaceHandle) -> Self {
        Self { package }
    }

    pub fn get_package(&self) -> PackageInterfaceHandle {
        self.package.clone()
    }

    pub fn format(package: PackageInterfaceHandle, _lock: bool) -> String {
        format!(
            "Removing <info>{}</info> (<comment>{}</comment>)",
            package.get_pretty_name(),
            package.get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev),
        )
    }
}

impl SolverOperation for UninstallOperation {
    const TYPE: &'static str = "uninstall";
}

impl OperationInterface for UninstallOperation {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_operation_type(&self) -> String {
        Self::TYPE.to_string()
    }

    fn show(&self, lock: bool) -> String {
        Self::format(self.package.clone(), lock)
    }

    fn as_uninstall_operation(&self) -> Option<&UninstallOperation> {
        Some(self)
    }

    fn get_package(&self) -> PackageInterfaceHandle {
        self.package.clone()
    }
}

impl std::fmt::Display for UninstallOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.show(false))
    }
}
