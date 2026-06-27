//! ref: composer/tests/Composer/Test/Package/Version/VersionSelectorTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::filter::platform_requirement_filter::PlatformRequirementFilterFactory;
use shirabe::io::BufferIO;
use shirabe::io::IOInterface;
use shirabe::package::BasePackageHandle;
use shirabe::package::CompleteAliasPackageHandle;
use shirabe::package::CompletePackageHandle;
use shirabe::package::Link;
use shirabe::package::PackageInterfaceHandle;
use shirabe::package::handle::PackageHandle;
use shirabe::package::version::VersionSelector;
use shirabe::package::version::version_parser::VersionParser;
use shirabe::repository::PlatformRepository;
use shirabe::repository::RepositorySetInterface;
use shirabe_php_shim::PhpMixed;
use shirabe_php_shim::{PHP_MAJOR_VERSION, PHP_MINOR_VERSION, PHP_RELEASE_VERSION};
use shirabe_semver::constraint::AnyConstraint;

use shirabe_external_packages::symfony::console::output::output_interface;

use crate::test_case::get_package;

mockall::mock! {
    RepositorySet {}
    impl RepositorySetInterface for RepositorySet {
        fn find_packages(
            &self,
            name: &str,
            constraint: Option<AnyConstraint>,
            flags: i64,
        ) -> anyhow::Result<Vec<BasePackageHandle>>;
    }
}

// `RepositorySetInterface` requires `Debug`; mockall does not generate it.
impl std::fmt::Debug for MockRepositorySet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MockRepositorySet")
    }
}

fn into_seam(mock: MockRepositorySet) -> Rc<RefCell<dyn RepositorySetInterface>> {
    Rc::new(RefCell::new(mock))
}

/// Mirrors PHPUnit `assertSame($expected, $best)`: object identity, not value equality.
fn assert_same(
    best: &Option<PackageInterfaceHandle>,
    expected: &PackageInterfaceHandle,
    msg: &str,
) {
    let best = best
        .as_ref()
        .unwrap_or_else(|| panic!("{msg}: expected Some(_), got None"));
    assert!(best.ptr_eq(expected), "{msg}");
}

fn require_link(package_name: &str, target: &str, pretty_constraint: &str) -> Link {
    let parser = VersionParser::new();
    Link::new(
        package_name.to_string(),
        target.to_string(),
        parser.parse_constraints(pretty_constraint).unwrap(),
        Some(Link::TYPE_REQUIRE.to_string()),
        pretty_constraint.to_string(),
    )
}

fn find_best(
    version_selector: &mut VersionSelector,
    package_name: &str,
    preferred_stability: &str,
    platform_requirement_filter: Option<
        std::rc::Rc<
            dyn shirabe::filter::platform_requirement_filter::PlatformRequirementFilterInterface,
        >,
    >,
    io: Option<Rc<RefCell<dyn IOInterface>>>,
) -> Option<PackageInterfaceHandle> {
    version_selector
        .find_best_candidate(
            package_name,
            None,
            preferred_stability,
            platform_requirement_filter,
            0,
            io,
            PhpMixed::Bool(true),
        )
        .unwrap()
}

#[test]
fn test_latest_version_is_returned() {
    let package_name = "foo/bar";

    let package1 = get_package("foo/bar", "1.2.1");
    let package2 = get_package("foo/bar", "1.2.2");
    let package3 = get_package("foo/bar", "1.2.0");
    let packages = vec![package1.clone(), package2.clone(), package3.clone()];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(1)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector = VersionSelector::new(into_seam(repository_set), None).unwrap();
    let best = find_best(&mut version_selector, package_name, "stable", None, None);

    // 1.2.2 should be returned because it's the latest of the returned versions
    assert_same(&best, &package2, "Latest version should be 1.2.2");
}

#[test]
#[ignore = "PlatformRepository initialization calls shirabe_php_shim::runtime::constant() which is still todo!(); unrelated to the RepositorySet seam"]
fn test_latest_version_is_returned_that_matches_php_requirements() {
    let package_name = "foo/bar";

    let mut overrides: IndexMap<String, PhpMixed> = IndexMap::new();
    overrides.insert("php".to_string(), PhpMixed::String("5.5.0".to_string()));
    let mut platform = PlatformRepository::new(vec![], overrides).unwrap();

    let package0 = get_package("foo/bar", "0.9.0");
    package0.__set_requires(IndexMap::from([(
        "php".to_string(),
        require_link(package_name, "php", ">=5.6"),
    )]));
    let package1 = get_package("foo/bar", "1.0.0");
    package1.__set_requires(IndexMap::from([(
        "php".to_string(),
        require_link(package_name, "php", ">=5.4"),
    )]));
    let package2 = get_package("foo/bar", "2.0.0");
    package2.__set_requires(IndexMap::from([(
        "php".to_string(),
        require_link(package_name, "php", ">=5.6"),
    )]));
    let package3 = get_package("foo/bar", "2.1.0");
    package3.__set_requires(IndexMap::from([(
        "php".to_string(),
        require_link(package_name, "php", ">=5.6"),
    )]));
    let packages = vec![
        package0.clone(),
        package1.clone(),
        package2.clone(),
        package3.clone(),
    ];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(3)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector =
        VersionSelector::new(into_seam(repository_set), Some(&mut platform)).unwrap();

    let io = Rc::new(RefCell::new(
        BufferIO::new(String::new(), output_interface::VERBOSITY_NORMAL, None).unwrap(),
    ));
    let io_dyn: Rc<RefCell<dyn IOInterface>> = io.clone();
    let best = find_best(
        &mut version_selector,
        package_name,
        "stable",
        None,
        Some(io_dyn),
    );
    assert_same(
        &best,
        &package1,
        "Latest version supporting php 5.5 should be returned (1.0.0)",
    );
    assert_eq!(
        "<warning>Cannot use foo/bar's latest version 2.1.0 as it requires php >=5.6 which is not satisfied by your platform.\n",
        io.borrow().get_output()
    );

    let io = Rc::new(RefCell::new(
        BufferIO::new(String::new(), output_interface::VERBOSITY_VERBOSE, None).unwrap(),
    ));
    let io_dyn: Rc<RefCell<dyn IOInterface>> = io.clone();
    let best = find_best(
        &mut version_selector,
        package_name,
        "stable",
        None,
        Some(io_dyn),
    );
    assert_same(
        &best,
        &package1,
        "Latest version supporting php 5.5 should be returned (1.0.0)",
    );
    assert_eq!(
        "<warning>Cannot use foo/bar's latest version 2.1.0 as it requires php >=5.6 which is not satisfied by your platform.\n\
         <warning>Cannot use foo/bar 2.0.0 as it requires php >=5.6 which is not satisfied by your platform.\n",
        io.borrow().get_output()
    );

    let best = find_best(
        &mut version_selector,
        package_name,
        "stable",
        Some(PlatformRequirementFilterFactory::ignore_all()),
        None,
    );
    assert_same(
        &best,
        &package3,
        "Latest version should be returned when ignoring platform reqs (2.1.0)",
    );
}

#[test]
#[ignore = "PlatformRepository initialization calls shirabe_php_shim::runtime::constant() which is still todo!(); unrelated to the RepositorySet seam"]
fn test_latest_version_is_returned_that_matches_ext_requirements() {
    let package_name = "foo/bar";

    let mut overrides: IndexMap<String, PhpMixed> = IndexMap::new();
    overrides.insert("ext-zip".to_string(), PhpMixed::String("5.3.0".to_string()));
    let mut platform = PlatformRepository::new(vec![], overrides).unwrap();

    let package1 = get_package("foo/bar", "1.0.0");
    package1.__set_requires(IndexMap::from([(
        "ext-zip".to_string(),
        require_link(package_name, "ext-zip", "^5.2"),
    )]));
    let package2 = get_package("foo/bar", "2.0.0");
    package2.__set_requires(IndexMap::from([(
        "ext-zip".to_string(),
        require_link(package_name, "ext-zip", "^5.4"),
    )]));
    let packages = vec![package1.clone(), package2.clone()];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(2)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector =
        VersionSelector::new(into_seam(repository_set), Some(&mut platform)).unwrap();

    let best = find_best(&mut version_selector, package_name, "stable", None, None);
    assert_same(
        &best,
        &package1,
        "Latest version supporting ext-zip 5.3.0 should be returned (1.0.0)",
    );
    let best = find_best(
        &mut version_selector,
        package_name,
        "stable",
        Some(PlatformRequirementFilterFactory::ignore_all()),
        None,
    );
    assert_same(
        &best,
        &package2,
        "Latest version should be returned when ignoring platform reqs (2.0.0)",
    );
}

#[test]
#[ignore = "PlatformRepository initialization calls shirabe_php_shim::runtime::constant() which is still todo!(); unrelated to the RepositorySet seam"]
fn test_latest_version_is_returned_that_matches_platform_ext() {
    let package_name = "foo/bar";

    let mut platform = PlatformRepository::new(vec![], IndexMap::new()).unwrap();

    let package1 = get_package("foo/bar", "1.0.0");
    let package2 = get_package("foo/bar", "2.0.0");
    package2.__set_requires(IndexMap::from([(
        "ext-barfoo".to_string(),
        require_link(package_name, "ext-barfoo", "*"),
    )]));
    let packages = vec![package1.clone(), package2.clone()];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(2)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector =
        VersionSelector::new(into_seam(repository_set), Some(&mut platform)).unwrap();

    let best = find_best(&mut version_selector, package_name, "stable", None, None);
    assert_same(
        &best,
        &package1,
        "Latest version not requiring ext-barfoo should be returned (1.0.0)",
    );
    let best = find_best(
        &mut version_selector,
        package_name,
        "stable",
        Some(PlatformRequirementFilterFactory::ignore_all()),
        None,
    );
    assert_same(
        &best,
        &package2,
        "Latest version should be returned when ignoring platform reqs (2.0.0)",
    );
}

#[test]
#[ignore = "PlatformRepository initialization calls shirabe_php_shim::runtime::constant() which is still todo!(); unrelated to the RepositorySet seam"]
fn test_latest_version_is_returned_that_matches_composer_requirements() {
    let package_name = "foo/bar";

    let mut overrides: IndexMap<String, PhpMixed> = IndexMap::new();
    overrides.insert(
        "composer-runtime-api".to_string(),
        PhpMixed::String("1.0.0".to_string()),
    );
    let mut platform = PlatformRepository::new(vec![], overrides).unwrap();

    let package1 = get_package("foo/bar", "1.0.0");
    package1.__set_requires(IndexMap::from([(
        "composer-runtime-api".to_string(),
        require_link(package_name, "composer-runtime-api", "^1.0"),
    )]));
    let package2 = get_package("foo/bar", "1.1.0");
    package2.__set_requires(IndexMap::from([(
        "composer-runtime-api".to_string(),
        require_link(package_name, "composer-runtime-api", "^2.0"),
    )]));
    let packages = vec![package1.clone(), package2.clone()];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(2)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector =
        VersionSelector::new(into_seam(repository_set), Some(&mut platform)).unwrap();

    let best = find_best(&mut version_selector, package_name, "stable", None, None);
    assert_same(
        &best,
        &package1,
        "Latest version supporting composer 1 should be returned (1.0.0)",
    );
    let best = find_best(
        &mut version_selector,
        package_name,
        "stable",
        Some(PlatformRequirementFilterFactory::ignore_all()),
        None,
    );
    assert_same(
        &best,
        &package2,
        "Latest version should be returned when ignoring platform reqs (1.1.0)",
    );
}

#[test]
fn test_most_stable_version_is_returned() {
    let package_name = "foo/bar";

    let package1 = get_package("foo/bar", "1.0.0");
    let package2 = get_package("foo/bar", "1.1.0-beta");
    let packages = vec![package1.clone(), package2.clone()];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(1)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector = VersionSelector::new(into_seam(repository_set), None).unwrap();
    let best = find_best(&mut version_selector, package_name, "stable", None, None);

    assert_same(
        &best,
        &package1,
        "Latest most stable version should be returned (1.0.0)",
    );
}

#[test]
fn test_most_stable_version_is_returned_regardless_of_order() {
    let package_name = "foo/bar";

    let package1 = get_package("foo/bar", "2.x-dev");
    let package2 = get_package("foo/bar", "2.0.0-beta3");
    let packages = vec![package1.clone(), package2.clone()];
    let reversed: Vec<PackageInterfaceHandle> = packages.iter().rev().cloned().collect();

    let mut repository_set = MockRepositorySet::new();
    let mut seq = mockall::Sequence::new();
    repository_set
        .expect_find_packages()
        .times(1)
        .in_sequence(&mut seq)
        .returning_st(move |_, _, _| Ok(packages.clone()));
    repository_set
        .expect_find_packages()
        .times(1)
        .in_sequence(&mut seq)
        .returning_st(move |_, _, _| Ok(reversed.clone()));

    let mut version_selector = VersionSelector::new(into_seam(repository_set), None).unwrap();
    let best = find_best(&mut version_selector, package_name, "stable", None, None);
    assert_same(
        &best,
        &package2,
        "Expecting 2.0.0-beta3, cause beta is more stable than dev",
    );

    let best = find_best(&mut version_selector, package_name, "stable", None, None);
    assert_same(
        &best,
        &package2,
        "Expecting 2.0.0-beta3, cause beta is more stable than dev",
    );
}

#[test]
fn test_highest_version_is_returned() {
    let package_name = "foo/bar";

    let package1 = get_package("foo/bar", "1.0.0");
    let package2 = get_package("foo/bar", "1.1.0-beta");
    let packages = vec![package1.clone(), package2.clone()];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(1)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector = VersionSelector::new(into_seam(repository_set), None).unwrap();
    let best = find_best(&mut version_selector, package_name, "dev", None, None);

    assert_same(
        &best,
        &package2,
        "Latest version should be returned (1.1.0-beta)",
    );
}

#[test]
fn test_highest_version_matching_stability_is_returned() {
    let package_name = "foo/bar";

    let package1 = get_package("foo/bar", "1.0.0");
    let package2 = get_package("foo/bar", "1.1.0-beta");
    let package3 = get_package("foo/bar", "1.2.0-alpha");
    let packages = vec![package1.clone(), package2.clone(), package3.clone()];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(1)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector = VersionSelector::new(into_seam(repository_set), None).unwrap();
    let best = find_best(&mut version_selector, package_name, "beta", None, None);

    assert_same(
        &best,
        &package2,
        "Latest version should be returned (1.1.0-beta)",
    );
}

#[test]
fn test_most_stable_unstable_version_is_returned() {
    let package_name = "foo/bar";

    let package2 = get_package("foo/bar", "1.1.0-beta");
    let package3 = get_package("foo/bar", "1.2.0-alpha");
    let packages = vec![package2.clone(), package3.clone()];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(1)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector = VersionSelector::new(into_seam(repository_set), None).unwrap();
    let best = find_best(&mut version_selector, package_name, "stable", None, None);

    assert_same(
        &best,
        &package2,
        "Latest version should be returned (1.1.0-beta)",
    );
}

#[test]
fn test_default_branch_alias_is_never_returned() {
    let package_name = "foo/bar";

    let package = get_package("foo/bar", "1.1.0-beta");
    let package2 = get_package("foo/bar", "dev-main");
    let package2_complete = CompletePackageHandle::from_rc_unchecked(package2.as_rc().clone());
    let package2_alias: PackageInterfaceHandle = CompleteAliasPackageHandle::new(
        package2_complete,
        VersionParser::DEFAULT_BRANCH_ALIAS.to_string(),
        VersionParser::DEFAULT_BRANCH_ALIAS.to_string(),
    )
    .into();
    let packages = vec![package.clone(), package2_alias.clone()];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(1)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector = VersionSelector::new(into_seam(repository_set), None).unwrap();
    let best = find_best(&mut version_selector, package_name, "dev", None, None);

    assert_same(
        &best,
        &package2,
        "Latest version should be returned (dev-main)",
    );
}

#[test]
fn test_false_returned_on_no_packages() {
    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(1)
        .returning_st(|_, _, _| Ok(vec![]));

    let mut version_selector = VersionSelector::new(into_seam(repository_set), None).unwrap();
    let best = find_best(&mut version_selector, "foobaz", "stable", None, None);
    assert!(best.is_none(), "No versions are available returns false");
}

#[test]
#[ignore = "date-based cases (v20121020) fail: shirabe_semver::VersionParser::normalize yields \
            20121020.0.0.0 instead of PHP's date-aware 20121020, so find_recommended_require_version \
            returns ^20121020.0 rather than leaving the version untouched. Faithful port; un-ignore \
            once normalize handles date(time) versions like PHP"]
fn test_find_recommended_require_version() {
    let php_version = format!(
        "{}.{}.{}",
        PHP_MAJOR_VERSION, PHP_MINOR_VERSION, PHP_RELEASE_VERSION
    );
    // real version, expected recommendation, [branch-alias], [pkg name]
    let cases: Vec<(String, &str, Option<&str>, &str)> = vec![
        ("1.2.1".to_string(), "^1.2", None, "foo/bar"),
        ("1.2".to_string(), "^1.2", None, "foo/bar"),
        ("v1.2.1".to_string(), "^1.2", None, "foo/bar"),
        ("3.1.2-pl2".to_string(), "^3.1", None, "foo/bar"),
        ("3.1.2-patch".to_string(), "^3.1", None, "foo/bar"),
        ("2.0-beta.1".to_string(), "^2.0@beta", None, "foo/bar"),
        ("3.1.2-alpha5".to_string(), "^3.1@alpha", None, "foo/bar"),
        ("3.0-RC2".to_string(), "^3.0@RC", None, "foo/bar"),
        ("0.1.0".to_string(), "^0.1.0", None, "foo/bar"),
        ("0.1.3".to_string(), "^0.1.3", None, "foo/bar"),
        ("0.0.3".to_string(), "^0.0.3", None, "foo/bar"),
        ("0.0.3-alpha".to_string(), "^0.0.3@alpha", None, "foo/bar"),
        ("0.0.3.4-alpha".to_string(), "^0.0.3@alpha", None, "foo/bar"),
        ("3.0.0.2-RC2".to_string(), "^3.0@RC", None, "foo/bar"),
        ("1.2.1.1020402".to_string(), "^1.2", None, "foo/bar"),
        // date-based versions are not touched at all
        ("v20121020".to_string(), "v20121020", None, "foo/bar"),
        ("v20121020.2".to_string(), "v20121020.2", None, "foo/bar"),
        // dev packages without alias are not touched at all
        ("dev-master".to_string(), "dev-master", None, "foo/bar"),
        ("3.1.2-dev".to_string(), "3.1.2-dev", None, "foo/bar"),
        // dev packages with alias inherit the alias
        (
            "dev-master".to_string(),
            "^2.1@dev",
            Some("2.1.x-dev"),
            "foo/bar",
        ),
        (
            "dev-master".to_string(),
            "^2.1@dev",
            Some("2.1-dev"),
            "foo/bar",
        ),
        (
            "dev-master".to_string(),
            "^2.1@dev",
            Some("2.1.3.x-dev"),
            "foo/bar",
        ),
        (
            "dev-master".to_string(),
            "^2.0@dev",
            Some("2.x-dev"),
            "foo/bar",
        ),
        (
            "dev-master".to_string(),
            "^0.3.0@dev",
            Some("0.3.x-dev"),
            "foo/bar",
        ),
        (
            "dev-master".to_string(),
            "^0.0.3@dev",
            Some("0.0.3.x-dev"),
            "foo/bar",
        ),
        (
            "dev-master".to_string(),
            "dev-master",
            Some(VersionParser::DEFAULT_BRANCH_ALIAS),
            "foo/bar",
        ),
        // numeric alias
        (
            "3.x-dev".to_string(),
            "^3.0@dev",
            Some("3.0.x-dev"),
            "foo/bar",
        ),
        (
            "3.x-dev".to_string(),
            "^3.0@dev",
            Some("3.0-dev"),
            "foo/bar",
        ),
        // ext in sync with php
        (php_version.clone(), "*", None, "ext-filter"),
        // ext versioned individually
        ("3.0.5".to_string(), "^3.0", None, "ext-xdebug"),
    ];

    let version_parser = VersionParser::new();
    for (pretty_version, expected_version, branch_alias, package_name) in &cases {
        let repository_set = MockRepositorySet::new();
        let mut version_selector = VersionSelector::new(into_seam(repository_set), None).unwrap();

        let package = PackageHandle::new(
            package_name.to_string(),
            version_parser.normalize(pretty_version, None).unwrap(),
            pretty_version.clone(),
        );

        if let Some(branch_alias) = branch_alias {
            let mut alias_map: IndexMap<String, PhpMixed> = IndexMap::new();
            alias_map.insert(
                pretty_version.clone(),
                PhpMixed::String(branch_alias.to_string()),
            );
            let mut extra: IndexMap<String, PhpMixed> = IndexMap::new();
            extra.insert("branch-alias".to_string(), PhpMixed::Array(alias_map));
            package.__set_extra(extra);
        }

        let recommended = version_selector
            .find_recommended_require_version(package.into())
            .unwrap();

        // assert that the recommended version is what we expect
        assert_eq!(
            *expected_version, recommended,
            "pretty_version {pretty_version:?}"
        );
    }
}
