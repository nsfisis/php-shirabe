//! ref: composer/src/Composer/DependencyResolver/Operation/OperationInterface.php
//!
//! PHP's `OperationInterface` is not ported as a trait. Operations are only ever constructed by the
//! dependency resolver, so a plugin has no way to inject an implementation of its own and the set is
//! closed. The shared implementations PHP puts on `SolverOperation` (`getOperationType()`,
//! `__toString()`) live here.

use crate::dependency_resolver::operation::{
    InstallOperation, MarkAliasInstalledOperation, MarkAliasUninstalledOperation, SolverOperation,
    UninstallOperation, UpdateOperation,
};
use crate::package::PackageInterfaceHandle;

/// Any solver operation.
#[derive(Debug, Clone)]
pub enum AnyOperation {
    Install(InstallOperation),
    Update(UpdateOperation),
    Uninstall(UninstallOperation),
    MarkAliasInstalled(MarkAliasInstalledOperation),
    MarkAliasUninstalled(MarkAliasUninstalledOperation),
}

impl AnyOperation {
    pub fn get_operation_type(&self) -> &'static str {
        match self {
            Self::Install(_) => InstallOperation::TYPE,
            Self::Update(_) => UpdateOperation::TYPE,
            Self::Uninstall(_) => UninstallOperation::TYPE,
            Self::MarkAliasInstalled(_) => MarkAliasInstalledOperation::TYPE,
            Self::MarkAliasUninstalled(_) => MarkAliasUninstalledOperation::TYPE,
        }
    }

    pub fn show(&self, lock: bool) -> String {
        match self {
            Self::Install(op) => op.show(lock),
            Self::Update(op) => op.show(lock),
            Self::Uninstall(op) => op.show(lock),
            Self::MarkAliasInstalled(op) => op.show(lock),
            Self::MarkAliasUninstalled(op) => op.show(lock),
        }
    }

    /// The package the operation results in. PHP spells this out at every call site as
    /// `$op instanceof UpdateOperation ? $op->getTargetPackage() : $op->getPackage()`.
    pub fn get_target_package(&self) -> PackageInterfaceHandle {
        match self {
            Self::Install(op) => op.get_package(),
            Self::Update(op) => op.get_target_package(),
            Self::Uninstall(op) => op.get_package(),
            Self::MarkAliasInstalled(op) => op.get_package().into(),
            Self::MarkAliasUninstalled(op) => op.get_package().into(),
        }
    }
}

impl std::fmt::Display for AnyOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.show(false))
    }
}

impl From<InstallOperation> for AnyOperation {
    fn from(op: InstallOperation) -> Self {
        Self::Install(op)
    }
}

impl From<UpdateOperation> for AnyOperation {
    fn from(op: UpdateOperation) -> Self {
        Self::Update(op)
    }
}

impl From<UninstallOperation> for AnyOperation {
    fn from(op: UninstallOperation) -> Self {
        Self::Uninstall(op)
    }
}

impl From<MarkAliasInstalledOperation> for AnyOperation {
    fn from(op: MarkAliasInstalledOperation) -> Self {
        Self::MarkAliasInstalled(op)
    }
}

impl From<MarkAliasUninstalledOperation> for AnyOperation {
    fn from(op: MarkAliasUninstalledOperation) -> Self {
        Self::MarkAliasUninstalled(op)
    }
}
