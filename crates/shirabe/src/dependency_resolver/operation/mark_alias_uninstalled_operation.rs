//! ref: composer/src/Composer/DependencyResolver/Operation/MarkAliasUninstalledOperation.php

use crate::dependency_resolver::operation::operation_interface::OperationInterface;
use crate::dependency_resolver::operation::solver_operation::SolverOperation;
use crate::package::alias_package::AliasPackage;
use crate::package::package_interface::PackageInterface;

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
            PackageInterface::get_pretty_name(&self.package),
            PackageInterface::get_full_pretty_version(
                &self.package,
                true,
                <dyn PackageInterface>::DISPLAY_SOURCE_REF_IF_DEV,
            ),
            PackageInterface::get_pretty_name(self.package.get_alias_of()),
            PackageInterface::get_full_pretty_version(
                self.package.get_alias_of(),
                true,
                <dyn PackageInterface>::DISPLAY_SOURCE_REF_IF_DEV,
            ),
        )
    }

    fn to_string(&self) -> String {
        self.show(true)
    }
}
