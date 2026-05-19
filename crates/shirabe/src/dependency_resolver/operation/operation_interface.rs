//! ref: composer/src/Composer/DependencyResolver/Operation/OperationInterface.php

use crate::dependency_resolver::operation::install_operation::InstallOperation;
use crate::dependency_resolver::operation::uninstall_operation::UninstallOperation;
use crate::dependency_resolver::operation::update_operation::UpdateOperation;

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
}
