//! ref: composer/src/Composer/DependencyResolver/PolicyInterface.php sha1:c00bd3f9db32608ffa0132a7c7b50c5b7980299a

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
