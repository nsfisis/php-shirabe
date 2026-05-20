//! ref: composer/src/Composer/DependencyResolver/Operation/OperationInterface.php

use crate::dependency_resolver::operation::InstallOperation;
use crate::dependency_resolver::operation::UninstallOperation;
use crate::dependency_resolver::operation::UpdateOperation;

pub trait OperationInterface: std::fmt::Debug {
    fn as_any(&self) -> &dyn std::any::Any;

    fn get_operation_type(&self) -> String;

    fn show(&self, lock: bool) -> String;

    fn to_string(&self) -> String;

    fn clone_box(&self) -> Box<dyn OperationInterface> {
        todo!()
    }

    fn as_install_operation(&self) -> Option<&InstallOperation> {
        None
    }

    fn as_update_operation(&self) -> Option<&UpdateOperation> {
        None
    }

    fn as_uninstall_operation(&self) -> Option<&UninstallOperation> {
        None
    }

    /// PHP duck-typed accessor. Only InstallOperation/UninstallOperation/MarkAlias*Operation
    /// expose this; UpdateOperation has getInitialPackage()/getTargetPackage() instead.
    fn get_package(&self) -> &dyn crate::package::PackageInterface {
        todo!("get_package is not available on this operation type")
    }
}
