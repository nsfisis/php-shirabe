//! ref: composer/src/Composer/DependencyResolver/Operation/OperationInterface.php

pub trait OperationInterface {
    fn get_operation_type(&self) -> String;

    fn show(&self, lock: bool) -> String;

    fn to_string(&self) -> String;
}
