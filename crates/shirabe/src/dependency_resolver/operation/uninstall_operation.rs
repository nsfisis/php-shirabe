//! ref: composer/src/Composer/DependencyResolver/Operation/UninstallOperation.php

use crate::dependency_resolver::operation::operation_interface::OperationInterface;
use crate::dependency_resolver::operation::solver_operation::SolverOperation;
use crate::package::package_interface::PackageInterface;

#[derive(Debug)]
pub struct UninstallOperation {
    pub(crate) package: Box<dyn PackageInterface>,
}

impl UninstallOperation {
    pub fn new(package: Box<dyn PackageInterface>) -> Self {
        Self { package }
    }

    pub fn get_package(&self) -> &dyn PackageInterface {
        self.package.as_ref()
    }

    pub fn format(package: &dyn PackageInterface, _lock: bool) -> String {
        format!(
            "Removing <info>{}</info> (<comment>{}</comment>)",
            package.get_pretty_name(),
            package
                .get_full_pretty_version(true, <dyn PackageInterface>::DISPLAY_SOURCE_REF_IF_DEV),
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
        Self::format(self.package.as_ref(), lock)
    }

    fn to_string(&self) -> String {
        self.show(true)
    }

    fn as_uninstall_operation(&self) -> Option<&UninstallOperation> {
        Some(self)
    }
}
