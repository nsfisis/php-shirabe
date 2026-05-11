//! ref: composer/src/Composer/DependencyResolver/Operation/MarkAliasUninstalledOperation.php

use crate::dependency_resolver::operation::operation_interface::OperationInterface;
use crate::dependency_resolver::operation::solver_operation::SolverOperation;
use crate::package::alias_package::AliasPackage;

#[derive(Debug)]
pub struct MarkAliasUninstalledOperation {
    pub(crate) package: AliasPackage,
}

impl MarkAliasUninstalledOperation {
    pub fn new(package: AliasPackage) -> Self {
        Self { package }
    }

    pub fn get_package(&self) -> &AliasPackage {
        &self.package
    }
}

impl SolverOperation for MarkAliasUninstalledOperation {
    const TYPE: &'static str = "markAliasUninstalled";
}

impl OperationInterface for MarkAliasUninstalledOperation {
    fn get_operation_type(&self) -> String {
        Self::TYPE.to_string()
    }

    fn show(&self, _lock: bool) -> String {
        format!(
            "Marking <info>{}</info> (<comment>{}</comment>) as uninstalled, alias of <info>{}</info> (<comment>{}</comment>)",
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
