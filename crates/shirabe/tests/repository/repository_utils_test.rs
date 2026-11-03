//! ref: composer/tests/Composer/Test/Repository/RepositoryUtilsTest.php

use crate::test_case::{get_alias_package, get_package};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe::package::loader::array_loader::ArrayLoader;
use shirabe::repository::RepositoryUtils;
use shirabe_php_shim::{PhpMixed, bconcat};

/// PHP `configureLinks` sets link arrays on non-root packages; the public handle API only allows
/// link setters on root packages, so packages carrying links are built via ArrayLoader.
fn load_with(
    name: &BStr,
    version: &BStr,
    link_type: &BStr,
    deps: Vec<(&BStr, &BStr)>,
) -> PackageInterfaceHandle {
    let mut config: IndexMap<BString, PhpMixed> = IndexMap::new();
    config.insert("name".into(), PhpMixed::string(bconcat!(name)));
    config.insert("version".into(), PhpMixed::string(bconcat!(version)));

    let mut links: IndexMap<BString, PhpMixed> = IndexMap::new();
    for (dep, constraint) in deps {
        links.insert(bconcat!(dep), PhpMixed::string(bconcat!(constraint)));
    }
    config.insert(bconcat!(link_type), PhpMixed::Array(links));

    ArrayLoader::new(None, false)
        .load_packages(vec![config])
        .unwrap()
        .remove(0)
}

/// ref: RepositoryUtilsTest::getPackages
fn build_packages() -> IndexMap<BString, PackageInterfaceHandle> {
    let package_c = get_package("required/c", "1.0.0");
    let package_c_alias = get_alias_package(&package_c, "2.0.0");

    let mut pkgs: IndexMap<BString, PackageInterfaceHandle> = IndexMap::new();
    pkgs.insert(BString::from("0"), get_package("dummy/pkg", "1.0.0"));
    pkgs.insert(BString::from("1"), get_package("dummy/pkg2", "2.0.0"));
    pkgs.insert(BString::from("a"), get_package("required/a", "1.0.0"));
    pkgs.insert(
        BString::from("b"),
        load_with(
            BStr::new("required/b"),
            BStr::new("1.0.0"),
            BStr::new("require"),
            vec![(BStr::new("required/c"), BStr::new("*"))],
        ),
    );
    pkgs.insert(BString::from("c"), package_c);
    pkgs.insert(BString::from("c-alias"), package_c_alias);
    pkgs.insert(
        BString::from("circular"),
        load_with(
            BStr::new("required/circular"),
            BStr::new("1.0.0"),
            BStr::new("require"),
            vec![(BStr::new("required/circular-b"), BStr::new("*"))],
        ),
    );
    pkgs.insert(
        BString::from("circular-b"),
        load_with(
            BStr::new("required/circular-b"),
            BStr::new("1.0.0"),
            BStr::new("require"),
            vec![(BStr::new("required/circular"), BStr::new("*"))],
        ),
    );
    pkgs
}

struct FilterCase {
    requirer: PackageInterfaceHandle,
    expected: Vec<&'static BStr>,
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
                BStr::new("requirer/pkg"),
                BStr::new("1.0.0"),
                BStr::new("require-dev"),
                vec![(BStr::new("required/a"), BStr::new("*"))],
            ),
            expected: vec![],
            include_require_dev: false,
        },
        // 'require-dev works if called with it enabled'
        FilterCase {
            requirer: load_with(
                BStr::new("requirer/pkg"),
                BStr::new("1.0.0"),
                BStr::new("require-dev"),
                vec![(BStr::new("required/a"), BStr::new("*"))],
            ),
            expected: vec![BStr::new("a")],
            include_require_dev: true,
        },
        // 'simple require'
        FilterCase {
            requirer: load_with(
                BStr::new("requirer/pkg"),
                BStr::new("1.0.0"),
                BStr::new("require"),
                vec![(BStr::new("required/a"), BStr::new("*"))],
            ),
            expected: vec![BStr::new("a")],
            include_require_dev: false,
        },
        // 'require constraint is irrelevant'
        FilterCase {
            requirer: load_with(
                BStr::new("requirer/pkg"),
                BStr::new("1.0.0"),
                BStr::new("require"),
                vec![(BStr::new("required/a"), BStr::new("dev-lala"))],
            ),
            expected: vec![BStr::new("a")],
            include_require_dev: false,
        },
        // 'require transitive deps and aliases are included'
        FilterCase {
            requirer: load_with(
                BStr::new("requirer/pkg"),
                BStr::new("1.0.0"),
                BStr::new("require"),
                vec![(BStr::new("required/b"), BStr::new("*"))],
            ),
            expected: vec![BStr::new("b"), BStr::new("c"), BStr::new("c-alias")],
            include_require_dev: false,
        },
        // 'circular deps are no problem'
        FilterCase {
            requirer: load_with(
                BStr::new("requirer/pkg"),
                BStr::new("1.0.0"),
                BStr::new("require"),
                vec![(BStr::new("required/circular"), BStr::new("*"))],
            ),
            expected: vec![BStr::new("circular"), BStr::new("circular-b")],
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
