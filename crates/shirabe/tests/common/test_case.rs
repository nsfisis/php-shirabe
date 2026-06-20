//! Shared test helpers ported from composer/tests/Composer/Test/TestCase.php.
//!
//! Included into each integration-test binary that needs them via
//! `#[path = "../common/test_case.rs"] mod test_case;`.
#![allow(dead_code)]

use shirabe::package::handle::{
    CompleteAliasPackageHandle, CompletePackageHandle, PackageInterfaceHandle,
};
use shirabe_semver::constraint::{AnyConstraint, SimpleConstraint};
use shirabe_semver::version_parser::VersionParser;

/// ref: TestCase::getPackage (default class CompletePackage)
pub fn get_package(name: &str, version: &str) -> PackageInterfaceHandle {
    let norm_version = VersionParser.normalize(version, None).unwrap();
    CompletePackageHandle::new(name.to_string(), norm_version, version.to_string()).into()
}

/// ref: TestCase::getAliasPackage (default class CompleteAliasPackage)
pub fn get_alias_package(
    package: &PackageInterfaceHandle,
    version: &str,
) -> PackageInterfaceHandle {
    let norm_version = VersionParser.normalize(version, None).unwrap();
    let complete = CompletePackageHandle::from_rc_unchecked(package.as_rc().clone());
    CompleteAliasPackageHandle::new(complete, norm_version, version.to_string()).into()
}

/// ref: TestCase::getVersionConstraint
pub fn get_version_constraint(operator: &str, version: &str) -> AnyConstraint {
    let normalized = VersionParser.normalize(version, None).unwrap();
    AnyConstraint::Simple(SimpleConstraint::new(
        operator.to_string(),
        normalized,
        Some(format!("{} {}", operator, version)),
    ))
}
