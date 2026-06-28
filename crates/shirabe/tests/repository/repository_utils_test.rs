//! ref: composer/tests/Composer/Test/Repository/RepositoryUtilsTest.php

use crate::test_case::{get_alias_package, get_package};
use indexmap::IndexMap;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe::package::loader::array_loader::ArrayLoader;
use shirabe::repository::RepositoryUtils;
use shirabe_php_shim::PhpMixed;

/// PHP `configureLinks` sets link arrays on non-root packages; the public handle API only allows
/// link setters on root packages, so packages carrying links are built via ArrayLoader.
fn load_with(
    name: &str,
    version: &str,
    link_type: &str,
    deps: Vec<(&str, &str)>,
) -> PackageInterfaceHandle {
    let mut config: IndexMap<String, PhpMixed> = IndexMap::new();
    config.insert("name".to_string(), PhpMixed::String(name.to_string()));
    config.insert("version".to_string(), PhpMixed::String(version.to_string()));

    let mut links: IndexMap<String, PhpMixed> = IndexMap::new();
    for (dep, constraint) in deps {
        links.insert(dep.to_string(), PhpMixed::String(constraint.to_string()));
    }
    config.insert(link_type.to_string(), PhpMixed::Array(links));

    ArrayLoader::new(None, false)
        .load_packages(vec![config])
        .unwrap()
        .remove(0)
}

/// ref: RepositoryUtilsTest::getPackages
fn build_packages() -> IndexMap<String, PackageInterfaceHandle> {
    let package_c = get_package("required/c", "1.0.0");
    let package_c_alias = get_alias_package(&package_c, "2.0.0");

    let mut pkgs: IndexMap<String, PackageInterfaceHandle> = IndexMap::new();
    pkgs.insert("0".to_string(), get_package("dummy/pkg", "1.0.0"));
    pkgs.insert("1".to_string(), get_package("dummy/pkg2", "2.0.0"));
    pkgs.insert("a".to_string(), get_package("required/a", "1.0.0"));
    pkgs.insert(
        "b".to_string(),
        load_with("required/b", "1.0.0", "require", vec![("required/c", "*")]),
    );
    pkgs.insert("c".to_string(), package_c);
    pkgs.insert("c-alias".to_string(), package_c_alias);
    pkgs.insert(
        "circular".to_string(),
        load_with(
            "required/circular",
            "1.0.0",
            "require",
            vec![("required/circular-b", "*")],
        ),
    );
    pkgs.insert(
        "circular-b".to_string(),
        load_with(
            "required/circular-b",
            "1.0.0",
            "require",
            vec![("required/circular", "*")],
        ),
    );
    pkgs
}

struct FilterCase {
    requirer: PackageInterfaceHandle,
    expected: Vec<&'static str>,
    include_require_dev: bool,
}

/// ref: RepositoryUtilsTest::provideFilterRequireTests
fn provide_filter_require_tests() -> Vec<FilterCase> {
    vec![
        // 'no require'
        FilterCase {
            requirer: get_package("requirer/pkg", "1.0.0"),
            expected: vec![],
            include_require_dev: false,
        },
        // 'require-dev has no effect'
        FilterCase {
            requirer: load_with(
                "requirer/pkg",
                "1.0.0",
                "require-dev",
                vec![("required/a", "*")],
            ),
            expected: vec![],
            include_require_dev: false,
        },
        // 'require-dev works if called with it enabled'
        FilterCase {
            requirer: load_with(
                "requirer/pkg",
                "1.0.0",
                "require-dev",
                vec![("required/a", "*")],
            ),
            expected: vec!["a"],
            include_require_dev: true,
        },
        // 'simple require'
        FilterCase {
            requirer: load_with(
                "requirer/pkg",
                "1.0.0",
                "require",
                vec![("required/a", "*")],
            ),
            expected: vec!["a"],
            include_require_dev: false,
        },
        // 'require constraint is irrelevant'
        FilterCase {
            requirer: load_with(
                "requirer/pkg",
                "1.0.0",
                "require",
                vec![("required/a", "dev-lala")],
            ),
            expected: vec!["a"],
            include_require_dev: false,
        },
        // 'require transitive deps and aliases are included'
        FilterCase {
            requirer: load_with(
                "requirer/pkg",
                "1.0.0",
                "require",
                vec![("required/b", "*")],
            ),
            expected: vec!["b", "c", "c-alias"],
            include_require_dev: false,
        },
        // 'circular deps are no problem'
        FilterCase {
            requirer: load_with(
                "requirer/pkg",
                "1.0.0",
                "require",
                vec![("required/circular", "*")],
            ),
            expected: vec!["circular", "circular-b"],
            include_require_dev: false,
        },
    ]
}

#[test]
fn test_filter_required_packages() {
    for case in provide_filter_require_tests() {
        let pkgs = build_packages();
        let packages: Vec<PackageInterfaceHandle> = pkgs.values().cloned().collect();
        let expected: Vec<PackageInterfaceHandle> = case
            .expected
            .iter()
            .map(|name| pkgs[*name].clone())
            .collect();

        let result = RepositoryUtils::filter_required_packages(
            &packages,
            case.requirer,
            case.include_require_dev,
            vec![],
        );

        assert_eq!(expected.len(), result.len());
        for (expected_pkg, result_pkg) in expected.iter().zip(result.iter()) {
            assert!(expected_pkg.ptr_eq(result_pkg));
        }
    }
}
