//! ref: composer/src/Composer/DependencyResolver/Operation/MarkAliasUninstalledOperation.php

use crate::dependency_resolver::operation::OperationInterface;
use crate::dependency_resolver::operation::SolverOperation;
use crate::package::AliasPackageHandle;

#[derive(Debug)]
pub struct MarkAliasUninstalledOperation {
    pub(crate) package: AliasPackageHandle,
}

impl MarkAliasUninstalledOperation {
    pub fn new(package: AliasPackageHandle) -> Self {
        Self { package }
    }

    pub fn get_package(&self) -> AliasPackageHandle {
        self.package.clone()
    }
}

impl SolverOperation for MarkAliasUninstalledOperation {
    const TYPE: &'static str = "markAliasUninstalled";
}

impl OperationInterface for MarkAliasUninstalledOperation {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_operation_type(&self) -> String {
        Self::TYPE.to_string()
    }

    fn show(&self, _lock: bool) -> String {
        format!(
            "Marking <info>{}</info> (<comment>{}</comment>) as uninstalled, alias of <info>{}</info> (<comment>{}</comment>)",
            self.package.get_pretty_name(),
            self.package
                .get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev),
            self.package.get_alias_of().get_pretty_name(),
            self.package
                .get_alias_of()
                .get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev),
        )
    }

    fn get_package(&self) -> crate::package::PackageInterfaceHandle {
        self.package.clone().into()
    }
}

impl std::fmt::Display for MarkAliasUninstalledOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.show(false))
    }
}
