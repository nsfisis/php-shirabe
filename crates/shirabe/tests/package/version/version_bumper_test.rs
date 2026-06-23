//! ref: composer/tests/Composer/Test/Package/Version/VersionBumperTest.php

use indexmap::IndexMap;
use shirabe::package::handle::{PackageHandle, PackageInterfaceHandle};
use shirabe::package::package::Package;
use shirabe::package::version::version_bumper::VersionBumper;
use shirabe_php_shim::PhpMixed;
use shirabe_semver::VersionParser;

#[test]
#[ignore]
fn test_bump_requirement() {
    for (requirement, pretty_version, expected_requirement, branch_alias) in
        provide_bump_requirement_tests()
    {
        let version_bumper = VersionBumper;
        let version_parser = VersionParser;

        let mut package = Package::new(
            "foo/bar".to_string(),
            version_parser.normalize(pretty_version, None).unwrap(),
            pretty_version.to_string(),
        );

        if let Some(branch_alias) = branch_alias {
            let mut alias_map: IndexMap<String, PhpMixed> = IndexMap::new();
            alias_map.insert(
                pretty_version.to_string(),
                PhpMixed::String(branch_alias.to_string()),
            );
            let mut extra: IndexMap<String, PhpMixed> = IndexMap::new();
            extra.insert("branch-alias".to_string(), PhpMixed::Array(alias_map));
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
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
)> {
    // constraint, version, expected recommendation, [branch-alias]
    vec![
        ("^1.0", "1.2.1", "^1.2.1", None),
        ("^v1.0", "1.2.1", "^1.2.1", None),
        ("^1.0", "1.0.0", "^1.0", None),
        ("^1.2", "1.2.0", "^1.2", None),
        ("^1.0.0", "1.2.0", "^1.2.0", None),
        ("^1.0.0", "1.2.1", "^1.2.1", None),
        ("^1.2 || ^2.3", "1.3.2", "^1.3.2 || ^2.3", None),
        ("^1.2 || ^2.3", "2.4.0", "^1.2 || ^2.4", None),
        ("^1.2 || ^2.3 || ^2", "2.4.0", "^1.2 || ^2.4 || ^2.4", None),
        (
            "^1.2 || ^2.3.3 || ^2",
            "2.4.0",
            "^1.2 || ^2.4.0 || ^2.4",
            None,
        ),
        ("^3@dev", "3.2.x-dev", "^3.2@dev", None),
        ("~2", "2.1-beta.1", "~2", None),
        ("dev-main", "dev-foo", "dev-main", None),
        ("^3.2", "dev-main", "^3.2", None),
        ("^3.2", "dev-main", "^3.3", Some("3.3.x-dev")),
        ("2.*", "2.4.0", "^2.4", None),
        ("v2.*", "2.4.0", "^2.4", None),
        ("2.x", "2.4.0", "^2.4", None),
        ("2.x.x", "2.4.0", "^2.4.0", None),
        ("2.4.*", "2.4.3", "2.4.*", None),
        ("2.4.3.*", "2.4.3.2", "2.4.3.*", None),
        ("~2", "2.4.3", "~2", None),
        ("~2.2", "2.4.3", "^2.4.3", None),
        ("~2.2.3", "2.2.6.2", "~2.2.6", None),
        ("~2.2.3", "2.2.6", "~2.2.6", None),
        ("~2.0.0", "2.0.0", "~2.0.0", None),
        ("~2025.1.561", "2025.1.583", "~2025.1.583", None),
        ("~2.2.3.1", "2.2.4", "~2.2.4.0", None),
        ("~2.2.3.1", "2.2.4.0", "~2.2.4.0", None),
        ("~2.2.3.1", "2.2.4.5", "~2.2.4.5", None),
        (">=3.0", "3.4.5", ">=3.4.5", None),
        (">=v3.0", "3.4.5", ">=3.4.5", None),
        (">2.2.3", "2.2.6", ">2.2.3", None),
        ("^0.3 || ^0.4", "0.4.3", "^0.3 || ^0.4.3", None),
        ("*", "1.2.3", ">=1.2.3", None),
    ]
}
