//! ref: composer/tests/Composer/Test/DependencyResolver/RequestTest.php

use indexmap::IndexMap;
use shirabe::dependency_resolver::request::Request;
use shirabe::package::version::version_parser::VersionParser;
use shirabe_semver::constraint::{AnyConstraint, MatchAllConstraint, SimpleConstraint};

// AnyConstraint does not implement PartialEq (it lives in another crate), so
// the original assertEquals() on the requires map is reproduced by comparing
// the Debug representations.
//
// The original tests also build ArrayRepositories and add packages, but those
// are never read by the assertions (the Request does not consult them), so the
// repository setup is omitted here.

fn get_version_constraint(operator: &str, version: &str) -> AnyConstraint {
    let normalized = VersionParser::new().normalize(version, None).unwrap();
    AnyConstraint::Simple(SimpleConstraint::new(
        operator.to_string(),
        normalized,
        Some(format!("{} {}", operator, version)),
    ))
}

#[test]
fn test_request_install() {
    let mut request = Request::new(None);
    request.require_name("foo", None).unwrap();

    let mut expected: IndexMap<String, AnyConstraint> = IndexMap::new();
    expected.insert("foo".to_string(), MatchAllConstraint::new(None).into());

    assert_eq!(
        format!("{:?}", expected),
        format!("{:?}", request.get_requires())
    );
}

#[test]
fn test_request_install_same_package_from_different_repositories() {
    let constraint = get_version_constraint("=", "1");

    let mut request = Request::new(None);
    request
        .require_name("foo", Some(constraint.clone()))
        .unwrap();

    let mut expected: IndexMap<String, AnyConstraint> = IndexMap::new();
    expected.insert("foo".to_string(), constraint);

    assert_eq!(
        format!("{:?}", expected),
        format!("{:?}", request.get_requires())
    );
}
