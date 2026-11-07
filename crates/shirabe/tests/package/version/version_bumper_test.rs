//! ref: composer/tests/Composer/Test/Package/Version/VersionBumperTest.php sha1:e31209bb4527aabf134daa2b14f52885f91cf8c5

use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::package::version::VersionBumper;
use shirabe::package::{Package, PackageHandle, PackageInterfaceHandle};
use shirabe_php_shim::{PhpMixed, ToPhpString};
use shirabe_semver::VersionParser;

#[test]
fn test_bump_requirement() {
    for (requirement, pretty_version, expected_requirement, branch_alias) in
        provide_bump_requirement_tests()
    {
        let version_bumper = VersionBumper;
        let version_parser = VersionParser;

        let mut package = Package::new(
            BString::from("foo/bar"),
            bstr::BString::from(&version_parser.normalize(pretty_version, None).unwrap()[..]),
            pretty_version.to_php_string(),
        );

        if let Some(branch_alias) = branch_alias {
            let mut alias_map: IndexMap<BString, PhpMixed> = IndexMap::new();
            alias_map.insert(
                pretty_version.to_php_string(),
                PhpMixed::string(branch_alias.to_php_string()),
            );
            let mut extra: IndexMap<BString, PhpMixed> = IndexMap::new();
            extra.insert("branch-alias".into(), PhpMixed::Array(alias_map));
            package.set_extra(extra);
        }

        let handle: PackageInterfaceHandle = PackageHandle::from_package(package).into();

        let new_constraint = version_bumper
            .bump_requirement(
                &version_parser.parse_constraints(requirement).unwrap(),
                handle,
            )
            .unwrap();

        // assert that the recommended version is what we expect
        assert_eq!(expected_requirement, new_constraint);
    }
}

fn provide_bump_requirement_tests() -> Vec<(
    &'static BStr,
    &'static BStr,
    &'static BStr,
    Option<&'static BStr>,
)> {
    // constraint, version, expected recommendation, [branch-alias]
    vec![
        (
            BStr::new("^1.0"),
            BStr::new("1.2.1"),
            BStr::new("^1.2.1"),
            None,
        ),
        (
            BStr::new("^v1.0"),
            BStr::new("1.2.1"),
            BStr::new("^1.2.1"),
            None,
        ),
        (
            BStr::new("^1.0"),
            BStr::new("1.0.0"),
            BStr::new("^1.0"),
            None,
        ),
        (
            BStr::new("^1.2"),
            BStr::new("1.2.0"),
            BStr::new("^1.2"),
            None,
        ),
        (
            BStr::new("^1.0.0"),
            BStr::new("1.2.0"),
            BStr::new("^1.2.0"),
            None,
        ),
        (
            BStr::new("^1.0.0"),
            BStr::new("1.2.1"),
            BStr::new("^1.2.1"),
            None,
        ),
        (
            BStr::new("^1.2 || ^2.3"),
            BStr::new("1.3.2"),
            BStr::new("^1.3.2 || ^2.3"),
            None,
        ),
        (
            BStr::new("^1.2 || ^2.3"),
            BStr::new("2.4.0"),
            BStr::new("^1.2 || ^2.4"),
            None,
        ),
        (
            BStr::new("^1.2 || ^2.3 || ^2"),
            BStr::new("2.4.0"),
            BStr::new("^1.2 || ^2.4 || ^2.4"),
            None,
        ),
        (
            BStr::new("^1.2 || ^2.3.3 || ^2"),
            BStr::new("2.4.0"),
            BStr::new("^1.2 || ^2.4.0 || ^2.4"),
            None,
        ),
        (
            BStr::new("^3@dev"),
            BStr::new("3.2.x-dev"),
            BStr::new("^3.2@dev"),
            None,
        ),
        (
            BStr::new("~2"),
            BStr::new("2.1-beta.1"),
            BStr::new("~2"),
            None,
        ),
        (
            BStr::new("dev-main"),
            BStr::new("dev-foo"),
            BStr::new("dev-main"),
            None,
        ),
        (
            BStr::new("^3.2"),
            BStr::new("dev-main"),
            BStr::new("^3.2"),
            None,
        ),
        (
            BStr::new("^3.2"),
            BStr::new("dev-main"),
            BStr::new("^3.3"),
            Some(BStr::new("3.3.x-dev")),
        ),
        (
            BStr::new("2.*"),
            BStr::new("2.4.0"),
            BStr::new("^2.4"),
            None,
        ),
        (
            BStr::new("v2.*"),
            BStr::new("2.4.0"),
            BStr::new("^2.4"),
            None,
        ),
        (
            BStr::new("2.x"),
            BStr::new("2.4.0"),
            BStr::new("^2.4"),
            None,
        ),
        (
            BStr::new("2.x.x"),
            BStr::new("2.4.0"),
            BStr::new("^2.4.0"),
            None,
        ),
        (
            BStr::new("2.4.*"),
            BStr::new("2.4.3"),
            BStr::new("2.4.*"),
            None,
        ),
        (
            BStr::new("2.4.3.*"),
            BStr::new("2.4.3.2"),
            BStr::new("2.4.3.*"),
            None,
        ),
        (BStr::new("~2"), BStr::new("2.4.3"), BStr::new("~2"), None),
        (
            BStr::new("~2.2"),
            BStr::new("2.4.3"),
            BStr::new("^2.4.3"),
            None,
        ),
        (
            BStr::new("~2.2.3"),
            BStr::new("2.2.6.2"),
            BStr::new("~2.2.6"),
            None,
        ),
        (
            BStr::new("~2.2.3"),
            BStr::new("2.2.6"),
            BStr::new("~2.2.6"),
            None,
        ),
        (
            BStr::new("~2.0.0"),
            BStr::new("2.0.0"),
            BStr::new("~2.0.0"),
            None,
        ),
        (
            BStr::new("~2025.1.561"),
            BStr::new("2025.1.583"),
            BStr::new("~2025.1.583"),
            None,
        ),
        (
            BStr::new("~2.2.3.1"),
            BStr::new("2.2.4"),
            BStr::new("~2.2.4.0"),
            None,
        ),
        (
            BStr::new("~2.2.3.1"),
            BStr::new("2.2.4.0"),
            BStr::new("~2.2.4.0"),
            None,
        ),
        (
            BStr::new("~2.2.3.1"),
            BStr::new("2.2.4.5"),
            BStr::new("~2.2.4.5"),
            None,
        ),
        (
            BStr::new(">=3.0"),
            BStr::new("3.4.5"),
            BStr::new(">=3.4.5"),
            None,
        ),
        (
            BStr::new(">=v3.0"),
            BStr::new("3.4.5"),
            BStr::new(">=3.4.5"),
            None,
        ),
        (
            BStr::new(">2.2.3"),
            BStr::new("2.2.6"),
            BStr::new(">2.2.3"),
            None,
        ),
        (
            BStr::new("^0.3 || ^0.4"),
            BStr::new("0.4.3"),
            BStr::new("^0.3 || ^0.4.3"),
            None,
        ),
        (
            BStr::new("*"),
            BStr::new("1.2.3"),
            BStr::new(">=1.2.3"),
            None,
        ),
    ]
}
