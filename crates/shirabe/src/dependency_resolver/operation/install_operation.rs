//! ref: composer/src/Composer/DependencyResolver/Operation/InstallOperation.php

use crate::dependency_resolver::operation::operation_interface::OperationInterface;
use crate::dependency_resolver::operation::solver_operation::SolverOperation;
use crate::package::package_interface::PackageInterface;

#[derive(Debug)]
pub struct InstallOperation {
    pub(crate) package: Box<dyn PackageInterface>,
}

impl InstallOperation {
    pub fn new(package: Box<dyn PackageInterface>) -> Self {
        Self { package }
    }

    pub fn get_package(&self) -> &dyn PackageInterface {
        self.package.as_ref()
    }

    pub fn format(package: &dyn PackageInterface, lock: bool) -> String {
        format!(
            "{}<info>{}</info> (<comment>{}</comment>)",
            if lock { "Locking " } else { "Installing " },
            package.get_pretty_name(),
            package.get_full_pretty_version(),
        )
    }
}

impl SolverOperation for InstallOperation {
    const TYPE: &'static str = "install";
}

impl OperationInterface for InstallOperation {
    fn get_operation_type(&self) -> String {
        Self::TYPE.to_string()
    }

    fn show(&self, lock: bool) -> String {
        Self::format(self.package.as_ref(), lock)
    }

    fn to_string(&self) -> String {
        self.show(true)
    }
}
