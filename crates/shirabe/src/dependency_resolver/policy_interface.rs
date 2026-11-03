//! ref: composer/src/Composer/DependencyResolver/PolicyInterface.php

use crate::dependency_resolver::Pool;
use crate::package::BasePackageHandle;
use bstr::{BStr, BString};

pub trait PolicyInterface: std::fmt::Debug {
    fn version_compare(&self, a: BasePackageHandle, b: BasePackageHandle, operator: &BStr) -> bool;

    fn select_preferred_packages(
        &self,
        pool: &Pool,
        literals: Vec<i64>,
        required_package: Option<BString>,
    ) -> Vec<i64>;
}
