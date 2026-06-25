//! ref: composer/src/Composer/DependencyResolver/Operation/MarkAliasInstalledOperation.php

use crate::dependency_resolver::operation::OperationInterface;
use crate::dependency_resolver::operation::SolverOperation;
use crate::package::AliasPackageHandle;

#[derive(Debug)]
pub struct MarkAliasInstalledOperation {
    pub(crate) package: AliasPackageHandle,
}

impl MarkAliasInstalledOperation {
    pub fn new(package: AliasPackageHandle) -> Self {
        Self { package }
    }

    pub fn get_package(&self) -> AliasPackageHandle {
        self.package.clone()
    }
}

impl SolverOperation for MarkAliasInstalledOperation {
    const TYPE: &'static str = "markAliasInstalled";
}

impl OperationInterface for MarkAliasInstalledOperation {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_operation_type(&self) -> String {
        Self::TYPE.to_string()
    }

    fn show(&self, _lock: bool) -> String {
        format!(
            "Marking <info>{}</info> (<comment>{}</comment>) as installed, alias of <info>{}</info> (<comment>{}</comment>)",
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

impl std::fmt::Display for MarkAliasInstalledOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.show(false))
    }
}
