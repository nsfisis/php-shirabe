//! ref: composer/src/Composer/DependencyResolver/Operation/MarkAliasInstalledOperation.php

use crate::dependency_resolver::operation::operation_interface::OperationInterface;
use crate::dependency_resolver::operation::solver_operation::SolverOperation;
use crate::package::alias_package::AliasPackage;

#[derive(Debug)]
pub struct MarkAliasInstalledOperation {
    pub(crate) package: AliasPackage,
}

impl MarkAliasInstalledOperation {
    pub fn new(package: AliasPackage) -> Self {
        Self { package }
    }

    pub fn get_package(&self) -> &AliasPackage {
        &self.package
    }
}

impl SolverOperation for MarkAliasInstalledOperation {
    const TYPE: &'static str = "markAliasInstalled";
}

impl OperationInterface for MarkAliasInstalledOperation {
    fn get_operation_type(&self) -> String {
        Self::TYPE.to_string()
    }

    fn show(&self, _lock: bool) -> String {
        format!(
            "Marking <info>{}</info> (<comment>{}</comment>) as installed, alias of <info>{}</info> (<comment>{}</comment>)",
            self.package.get_pretty_name(),
            self.package.get_full_pretty_version(),
            self.package.get_alias_of().get_pretty_name(),
            self.package.get_alias_of().get_full_pretty_version(),
        )
    }

    fn to_string(&self) -> String {
        self.show(true)
    }
}
