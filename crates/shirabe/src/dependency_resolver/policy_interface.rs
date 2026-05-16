//! ref: composer/src/Composer/DependencyResolver/PolicyInterface.php

use crate::dependency_resolver::pool::Pool;
use crate::package::package_interface::PackageInterface;

pub trait PolicyInterface {
    fn version_compare(
        &self,
        a: &dyn PackageInterface,
        b: &dyn PackageInterface,
        operator: &str,
    ) -> bool;

    fn select_preferred_packages(
        &self,
        pool: &Pool,
        literals: Vec<i64>,
        required_package: Option<String>,
    ) -> Vec<i64>;
}
