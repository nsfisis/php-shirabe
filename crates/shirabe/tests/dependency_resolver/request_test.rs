//! ref: composer/tests/Composer/Test/DependencyResolver/RequestTest.php sha1:6b4cb585c7f271227a686420bd50e678c036bb9b

use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::dependency_resolver::Request;
use shirabe::package::version::VersionParser;
use shirabe_php_shim::{bconcat, display_to_bytes};
use shirabe_semver::constraint::{AnyConstraint, MatchAllConstraint, SimpleConstraint};

// AnyConstraint does not implement PartialEq (it lives in another crate), so
// the original assertEquals() on the requires map is reproduced by comparing
// the Debug representations.
//
// The original tests also build ArrayRepositories and add packages, but those
// are never read by the assertions (the Request does not consult them), so the
// repository setup is omitted here.

fn get_version_constraint(operator: &BStr, version: &BStr) -> AnyConstraint {
    let normalized = VersionParser::new().normalize(version, None).unwrap();
    AnyConstraint::Simple(SimpleConstraint::new(
        operator,
        normalized,
        Some(bconcat!(operator, " ", version)),
    ))
}

#[test]
fn test_request_install() {
    let mut request = Request::new(None);
    request.require_name(bstr::BStr::new("foo"), None).unwrap();

    let mut expected: IndexMap<BString, AnyConstraint> = IndexMap::new();
    expected.insert(BString::from("foo"), MatchAllConstraint::new(None).into());

    assert_eq!(
        display_to_bytes(format_args!("{expected:?}")),
        display_to_bytes(format_args!("{:?}", request.get_requires()))
    );
}

#[test]
fn test_request_install_same_package_from_different_repositories() {
    let constraint = get_version_constraint(BStr::new("="), BStr::new("1"));

    let mut request = Request::new(None);
    request
        .require_name(bstr::BStr::new("foo"), Some(constraint.clone()))
        .unwrap();

    let mut expected: IndexMap<BString, AnyConstraint> = IndexMap::new();
    expected.insert(BString::from("foo"), constraint);

    assert_eq!(
        display_to_bytes(format_args!("{expected:?}")),
        display_to_bytes(format_args!("{:?}", request.get_requires()))
    );
}
