//! ref: composer/src/Composer/DependencyResolver/PolicyInterface.php

use crate::dependency_resolver::Pool;
use crate::package::BasePackageHandle;

pub trait PolicyInterface: std::fmt::Debug {
    fn version_compare(&self, a: BasePackageHandle, b: BasePackageHandle, operator: &str) -> bool;

    fn select_preferred_packages(
        &self,
        pool: &Pool,
        literals: Vec<i64>,
        required_package: Option<String>,
    ) -> Vec<i64>;

    fn clone_box(&self) -> Box<dyn PolicyInterface> {
        todo!()
    }
}
