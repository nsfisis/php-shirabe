//! ref: composer/src/Composer/DependencyResolver/Operation/SolverOperation.php

use crate::dependency_resolver::operation::operation_interface::OperationInterface;

pub trait SolverOperation: OperationInterface {
    const TYPE: &'static str;

    fn get_operation_type(&self) -> &str {
        Self::TYPE
    }
}
