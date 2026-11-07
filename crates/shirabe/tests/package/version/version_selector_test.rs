//! ref: composer/tests/Composer/Test/Package/Version/VersionSelectorTest.php sha1:ac392ad967c22aed7bc8ace70fe179b33e1e0179

use crate::test_case::get_package;
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe::filter::platform_requirement_filter::PlatformRequirementFilterFactory;
use shirabe::io::{BufferIO, IOInterface};
use shirabe::package::version::{VersionParser, VersionSelector};
use shirabe::package::{
    BasePackageHandle, CompleteAliasPackageHandle, CompletePackageHandle, Link, PackageHandle,
    PackageInterfaceHandle,
};
use shirabe::repository::{PlatformRepository, RepositorySetInterface};
use shirabe_php_shim::{PhpMixed, ToPhpString, bformat};
use shirabe_semver::constraint::AnyConstraint;
use shirabe_symfony_console::output::output_interface;

#[expect(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    reason = "the code mockall generates"
)]
mod mock_repository_set {
    use super::*;

    mockall::mock! {
        pub RepositorySet {}
        impl RepositorySetInterface for RepositorySet {
            fn find_packages(
                &self,
                name: &BStr,
                constraint: Option<AnyConstraint>,
                flags: i64,
            ) -> anyhow::Result<Vec<BasePackageHandle>>;
        }
    }
}
use mock_repository_set::MockRepositorySet;

// `RepositorySetInterface` requires `Debug`; mockall does not generate it.
impl std::fmt::Debug for MockRepositorySet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MockRepositorySet")
    }
}

fn into_seam(
    mock: MockRepositorySet,
) -> std::rc::Rc<std::cell::RefCell<dyn RepositorySetInterface>> {
    std::rc::Rc::new(std::cell::RefCell::new(mock))
}

/// Mirrors PHPUnit `assertSame($expected, $best)`: object identity, not value equality.
fn assert_same(
    best: &Option<PackageInterfaceHandle>,
    expected: &PackageInterfaceHandle,
    msg: &BStr,
) {
    let best = best.as_ref().unwrap_or_else(|| {
        panic!(
            "{msg}: expected Some(_), got None",
            msg = msg.display_lossy()
        )
    });
    assert!(best.ptr_eq(expected), "{msg}", msg = msg.display_lossy());
}

fn require_link(package_name: &BStr, target: &BStr, pretty_constraint: &BStr) -> Link {
    let parser = VersionParser::new();
    Link::new(
        package_name.to_php_string(),
        target.to_php_string(),
        parser.parse_constraints(pretty_constraint).unwrap(),
        Some(Link::TYPE_REQUIRE.to_owned()),
        Some(pretty_constraint.to_php_string()),
    )
}

fn find_best(
    version_selector: &mut VersionSelector,
    package_name: &BStr,
    preferred_stability: &BStr,
    platform_requirement_filter: Option<
        std::rc::Rc<
            dyn shirabe::filter::platform_requirement_filter::PlatformRequirementFilterInterface,
        >,
    >,
    io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
) -> Option<PackageInterfaceHandle> {
    version_selector
        .find_best_candidate(
            bstr::BStr::new(package_name),
            None,
            bstr::BStr::new(preferred_stability),
            platform_requirement_filter,
            0,
            io,
            shirabe::package::version::version_selector::ShowWarnings::Always,
        )
        .unwrap()
}

#[test]
fn test_latest_version_is_returned() {
    let package_name = BStr::new("foo/bar");

    let package1 = get_package("foo/bar", "1.2.1");
    let package2 = get_package("foo/bar", "1.2.2");
    let package3 = get_package("foo/bar", "1.2.0");
    let packages = vec![package1, package2.clone(), package3];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(1)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector = VersionSelector::new(into_seam(repository_set), None).unwrap();
    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("stable"),
        None,
        None,
    );

    // 1.2.2 should be returned because it's the latest of the returned versions
    assert_same(
        &best,
        &package2,
        BStr::new("Latest version should be 1.2.2"),
    );
}

#[test]
fn test_latest_version_is_returned_that_matches_php_requirements() {
    let package_name = BStr::new("foo/bar");

    let mut overrides: IndexMap<BString, PhpMixed> = IndexMap::new();
    overrides.insert("php".into(), PhpMixed::string(BString::from("5.5.0")));
    let mut platform = PlatformRepository::new(vec![], overrides, None, None).unwrap();

    let package0 = get_package("foo/bar", "0.9.0");
    package0.__set_requires(
        IndexMap::from([(
            BString::from("php"),
            require_link(package_name, BStr::new("php"), BStr::new(">=5.6")),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    let package1 = get_package("foo/bar", "1.0.0");
    package1.__set_requires(
        IndexMap::from([(
            BString::from("php"),
            require_link(package_name, BStr::new("php"), BStr::new(">=5.4")),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    let package2 = get_package("foo/bar", "2.0.0");
    package2.__set_requires(
        IndexMap::from([(
            BString::from("php"),
            require_link(package_name, BStr::new("php"), BStr::new(">=5.6")),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    let package3 = get_package("foo/bar", "2.1.0");
    package3.__set_requires(
        IndexMap::from([(
            BString::from("php"),
            require_link(package_name, BStr::new("php"), BStr::new(">=5.6")),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    let packages = vec![package0, package1.clone(), package2, package3.clone()];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(3)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector =
        VersionSelector::new(into_seam(repository_set), Some(&mut platform)).unwrap();

    let io = std::rc::Rc::new(std::cell::RefCell::new(
        BufferIO::new(BString::default(), output_interface::VERBOSITY_NORMAL, None).unwrap(),
    ));
    let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io.clone();
    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("stable"),
        None,
        Some(io_dyn),
    );
    assert_same(
        &best,
        &package1,
        BStr::new("Latest version supporting php 5.5 should be returned (1.0.0)"),
    );
    assert_eq!(
        "<warning>Cannot use foo/bar's latest version 2.1.0 as it requires php >=5.6 which is not satisfied by your platform.\n",
        io.borrow().get_output()
    );

    let io = std::rc::Rc::new(std::cell::RefCell::new(
        BufferIO::new(
            BString::default(),
            output_interface::VERBOSITY_VERBOSE,
            None,
        )
        .unwrap(),
    ));
    let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io.clone();
    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("stable"),
        None,
        Some(io_dyn),
    );
    assert_same(
        &best,
        &package1,
        BStr::new("Latest version supporting php 5.5 should be returned (1.0.0)"),
    );
    assert_eq!(
        "<warning>Cannot use foo/bar's latest version 2.1.0 as it requires php >=5.6 which is not satisfied by your platform.\n\
         <warning>Cannot use foo/bar 2.0.0 as it requires php >=5.6 which is not satisfied by your platform.\n",
        io.borrow().get_output()
    );

    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("stable"),
        Some(PlatformRequirementFilterFactory::ignore_all()),
        None,
    );
    assert_same(
        &best,
        &package3,
        BStr::new("Latest version should be returned when ignoring platform reqs (2.1.0)"),
    );
}

#[test]
fn test_latest_version_is_returned_that_matches_ext_requirements() {
    let package_name = BStr::new("foo/bar");

    let mut overrides: IndexMap<BString, PhpMixed> = IndexMap::new();
    overrides.insert("ext-zip".into(), PhpMixed::string(BString::from("5.3.0")));
    let mut platform = PlatformRepository::new(vec![], overrides, None, None).unwrap();

    let package1 = get_package("foo/bar", "1.0.0");
    package1.__set_requires(
        IndexMap::from([(
            BString::from("ext-zip"),
            require_link(package_name, BStr::new("ext-zip"), BStr::new("^5.2")),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    let package2 = get_package("foo/bar", "2.0.0");
    package2.__set_requires(
        IndexMap::from([(
            BString::from("ext-zip"),
            require_link(package_name, BStr::new("ext-zip"), BStr::new("^5.4")),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    let packages = vec![package1.clone(), package2.clone()];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(2)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector =
        VersionSelector::new(into_seam(repository_set), Some(&mut platform)).unwrap();

    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("stable"),
        None,
        None,
    );
    assert_same(
        &best,
        &package1,
        BStr::new("Latest version supporting ext-zip 5.3.0 should be returned (1.0.0)"),
    );
    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("stable"),
        Some(PlatformRequirementFilterFactory::ignore_all()),
        None,
    );
    assert_same(
        &best,
        &package2,
        BStr::new("Latest version should be returned when ignoring platform reqs (2.0.0)"),
    );
}

#[test]
fn test_latest_version_is_returned_that_matches_platform_ext() {
    let package_name = BStr::new("foo/bar");

    let mut platform = PlatformRepository::new(vec![], IndexMap::new(), None, None).unwrap();

    let package1 = get_package("foo/bar", "1.0.0");
    let package2 = get_package("foo/bar", "2.0.0");
    package2.__set_requires(
        IndexMap::from([(
            BString::from("ext-barfoo"),
            require_link(package_name, BStr::new("ext-barfoo"), BStr::new("*")),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    let packages = vec![package1.clone(), package2.clone()];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(2)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector =
        VersionSelector::new(into_seam(repository_set), Some(&mut platform)).unwrap();

    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("stable"),
        None,
        None,
    );
    assert_same(
        &best,
        &package1,
        BStr::new("Latest version not requiring ext-barfoo should be returned (1.0.0)"),
    );
    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("stable"),
        Some(PlatformRequirementFilterFactory::ignore_all()),
        None,
    );
    assert_same(
        &best,
        &package2,
        BStr::new("Latest version should be returned when ignoring platform reqs (2.0.0)"),
    );
}

#[test]
fn test_latest_version_is_returned_that_matches_composer_requirements() {
    let package_name = BStr::new("foo/bar");

    let mut overrides: IndexMap<BString, PhpMixed> = IndexMap::new();
    overrides.insert(
        "composer-runtime-api".into(),
        PhpMixed::string(BString::from("1.0.0")),
    );
    let mut platform = PlatformRepository::new(vec![], overrides, None, None).unwrap();

    let package1 = get_package("foo/bar", "1.0.0");
    package1.__set_requires(
        IndexMap::from([(
            BString::from("composer-runtime-api"),
            require_link(
                package_name,
                BStr::new("composer-runtime-api"),
                BStr::new("^1.0"),
            ),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    let package2 = get_package("foo/bar", "1.1.0");
    package2.__set_requires(
        IndexMap::from([(
            BString::from("composer-runtime-api"),
            require_link(
                package_name,
                BStr::new("composer-runtime-api"),
                BStr::new("^2.0"),
            ),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    let packages = vec![package1.clone(), package2.clone()];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(2)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector =
        VersionSelector::new(into_seam(repository_set), Some(&mut platform)).unwrap();

    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("stable"),
        None,
        None,
    );
    assert_same(
        &best,
        &package1,
        BStr::new("Latest version supporting composer 1 should be returned (1.0.0)"),
    );
    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("stable"),
        Some(PlatformRequirementFilterFactory::ignore_all()),
        None,
    );
    assert_same(
        &best,
        &package2,
        BStr::new("Latest version should be returned when ignoring platform reqs (1.1.0)"),
    );
}

#[test]
fn test_most_stable_version_is_returned() {
    let package_name = BStr::new("foo/bar");

    let package1 = get_package("foo/bar", "1.0.0");
    let package2 = get_package("foo/bar", "1.1.0-beta");
    let packages = vec![package1.clone(), package2];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(1)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector = VersionSelector::new(into_seam(repository_set), None).unwrap();
    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("stable"),
        None,
        None,
    );

    assert_same(
        &best,
        &package1,
        BStr::new("Latest most stable version should be returned (1.0.0)"),
    );
}

#[test]
fn test_most_stable_version_is_returned_regardless_of_order() {
    let package_name = BStr::new("foo/bar");

    let package1 = get_package("foo/bar", "2.x-dev");
    let package2 = get_package("foo/bar", "2.0.0-beta3");
    let packages = vec![package1, package2.clone()];
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
    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("stable"),
        None,
        None,
    );
    assert_same(
        &best,
        &package2,
        BStr::new("Expecting 2.0.0-beta3, cause beta is more stable than dev"),
    );

    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("stable"),
        None,
        None,
    );
    assert_same(
        &best,
        &package2,
        BStr::new("Expecting 2.0.0-beta3, cause beta is more stable than dev"),
    );
}

#[test]
fn test_highest_version_is_returned() {
    let package_name = BStr::new("foo/bar");

    let package1 = get_package("foo/bar", "1.0.0");
    let package2 = get_package("foo/bar", "1.1.0-beta");
    let packages = vec![package1, package2.clone()];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(1)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector = VersionSelector::new(into_seam(repository_set), None).unwrap();
    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("dev"),
        None,
        None,
    );

    assert_same(
        &best,
        &package2,
        BStr::new("Latest version should be returned (1.1.0-beta)"),
    );
}

#[test]
fn test_highest_version_matching_stability_is_returned() {
    let package_name = BStr::new("foo/bar");

    let package1 = get_package("foo/bar", "1.0.0");
    let package2 = get_package("foo/bar", "1.1.0-beta");
    let package3 = get_package("foo/bar", "1.2.0-alpha");
    let packages = vec![package1, package2.clone(), package3];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(1)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector = VersionSelector::new(into_seam(repository_set), None).unwrap();
    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("beta"),
        None,
        None,
    );

    assert_same(
        &best,
        &package2,
        BStr::new("Latest version should be returned (1.1.0-beta)"),
    );
}

#[test]
fn test_most_stable_unstable_version_is_returned() {
    let package_name = BStr::new("foo/bar");

    let package2 = get_package("foo/bar", "1.1.0-beta");
    let package3 = get_package("foo/bar", "1.2.0-alpha");
    let packages = vec![package2.clone(), package3];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(1)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector = VersionSelector::new(into_seam(repository_set), None).unwrap();
    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("stable"),
        None,
        None,
    );

    assert_same(
        &best,
        &package2,
        BStr::new("Latest version should be returned (1.1.0-beta)"),
    );
}

#[test]
fn test_default_branch_alias_is_never_returned() {
    let package_name = BStr::new("foo/bar");

    let package = get_package("foo/bar", "1.1.0-beta");
    let package2 = get_package("foo/bar", "dev-main");
    let package2_complete = CompletePackageHandle::from_rc_unchecked(package2.as_rc().clone());
    let package2_alias: PackageInterfaceHandle = CompleteAliasPackageHandle::new(
        package2_complete,
        VersionParser::DEFAULT_BRANCH_ALIAS.to_owned(),
        VersionParser::DEFAULT_BRANCH_ALIAS.to_owned(),
    )
    .unwrap()
    .into();
    let packages = vec![package, package2_alias];

    let mut repository_set = MockRepositorySet::new();
    repository_set
        .expect_find_packages()
        .times(1)
        .returning_st(move |_, _, _| Ok(packages.clone()));

    let mut version_selector = VersionSelector::new(into_seam(repository_set), None).unwrap();
    let best = find_best(
        &mut version_selector,
        package_name,
        BStr::new("dev"),
        None,
        None,
    );

    assert_same(
        &best,
        &package2,
        BStr::new("Latest version should be returned (dev-main)"),
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
    let best = find_best(
        &mut version_selector,
        BStr::new("foobaz"),
        BStr::new("stable"),
        None,
        None,
    );
    assert!(best.is_none(), "No versions are available returns false");
}

#[test]
fn test_find_recommended_require_version() {
    let php = shirabe_php_rpc::get_php_version().unwrap();
    let php_version = bformat!("{}.{}.{}", php.major, php.minor, php.release);
    // real version, expected recommendation, [branch-alias], [pkg name]
    let cases: Vec<(BString, &BStr, Option<&BStr>, &BStr)> = vec![
        (
            BString::from("1.2.1"),
            BStr::new("^1.2"),
            None,
            BStr::new("foo/bar"),
        ),
        (
            BString::from("1.2"),
            BStr::new("^1.2"),
            None,
            BStr::new("foo/bar"),
        ),
        (
            BString::from("v1.2.1"),
            BStr::new("^1.2"),
            None,
            BStr::new("foo/bar"),
        ),
        (
            BString::from("3.1.2-pl2"),
            BStr::new("^3.1"),
            None,
            BStr::new("foo/bar"),
        ),
        (
            BString::from("3.1.2-patch"),
            BStr::new("^3.1"),
            None,
            BStr::new("foo/bar"),
        ),
        (
            BString::from("2.0-beta.1"),
            BStr::new("^2.0@beta"),
            None,
            BStr::new("foo/bar"),
        ),
        (
            BString::from("3.1.2-alpha5"),
            BStr::new("^3.1@alpha"),
            None,
            BStr::new("foo/bar"),
        ),
        (
            BString::from("3.0-RC2"),
            BStr::new("^3.0@RC"),
            None,
            BStr::new("foo/bar"),
        ),
        (
            BString::from("0.1.0"),
            BStr::new("^0.1.0"),
            None,
            BStr::new("foo/bar"),
        ),
        (
            BString::from("0.1.3"),
            BStr::new("^0.1.3"),
            None,
            BStr::new("foo/bar"),
        ),
        (
            BString::from("0.0.3"),
            BStr::new("^0.0.3"),
            None,
            BStr::new("foo/bar"),
        ),
        (
            BString::from("0.0.3-alpha"),
            BStr::new("^0.0.3@alpha"),
            None,
            BStr::new("foo/bar"),
        ),
        (
            BString::from("0.0.3.4-alpha"),
            BStr::new("^0.0.3@alpha"),
            None,
            BStr::new("foo/bar"),
        ),
        (
            BString::from("3.0.0.2-RC2"),
            BStr::new("^3.0@RC"),
            None,
            BStr::new("foo/bar"),
        ),
        (
            BString::from("1.2.1.1020402"),
            BStr::new("^1.2"),
            None,
            BStr::new("foo/bar"),
        ),
        // date-based versions are not touched at all
        (
            BString::from("v20121020"),
            BStr::new("v20121020"),
            None,
            BStr::new("foo/bar"),
        ),
        (
            BString::from("v20121020.2"),
            BStr::new("v20121020.2"),
            None,
            BStr::new("foo/bar"),
        ),
        // dev packages without alias are not touched at all
        (
            BString::from("dev-master"),
            BStr::new("dev-master"),
            None,
            BStr::new("foo/bar"),
        ),
        (
            BString::from("3.1.2-dev"),
            BStr::new("3.1.2-dev"),
            None,
            BStr::new("foo/bar"),
        ),
        // dev packages with alias inherit the alias
        (
            BString::from("dev-master"),
            BStr::new("^2.1@dev"),
            Some(BStr::new("2.1.x-dev")),
            BStr::new("foo/bar"),
        ),
        (
            BString::from("dev-master"),
            BStr::new("^2.1@dev"),
            Some(BStr::new("2.1-dev")),
            BStr::new("foo/bar"),
        ),
        (
            BString::from("dev-master"),
            BStr::new("^2.1@dev"),
            Some(BStr::new("2.1.3.x-dev")),
            BStr::new("foo/bar"),
        ),
        (
            BString::from("dev-master"),
            BStr::new("^2.0@dev"),
            Some(BStr::new("2.x-dev")),
            BStr::new("foo/bar"),
        ),
        (
            BString::from("dev-master"),
            BStr::new("^0.3.0@dev"),
            Some(BStr::new("0.3.x-dev")),
            BStr::new("foo/bar"),
        ),
        (
            BString::from("dev-master"),
            BStr::new("^0.0.3@dev"),
            Some(BStr::new("0.0.3.x-dev")),
            BStr::new("foo/bar"),
        ),
        (
            BString::from("dev-master"),
            BStr::new("dev-master"),
            Some(VersionParser::DEFAULT_BRANCH_ALIAS),
            BStr::new("foo/bar"),
        ),
        // numeric alias
        (
            BString::from("3.x-dev"),
            BStr::new("^3.0@dev"),
            Some(BStr::new("3.0.x-dev")),
            BStr::new("foo/bar"),
        ),
        (
            BString::from("3.x-dev"),
            BStr::new("^3.0@dev"),
            Some(BStr::new("3.0-dev")),
            BStr::new("foo/bar"),
        ),
        // ext in sync with php
        (php_version, BStr::new("*"), None, BStr::new("ext-filter")),
        // ext versioned individually
        (
            BString::from("3.0.5"),
            BStr::new("^3.0"),
            None,
            BStr::new("ext-xdebug"),
        ),
    ];

    let version_parser = VersionParser::new();
    for (pretty_version, expected_version, branch_alias, package_name) in &cases {
        let repository_set = MockRepositorySet::new();
        let mut version_selector = VersionSelector::new(into_seam(repository_set), None).unwrap();

        let package = PackageHandle::new(
            package_name.to_php_string(),
            version_parser.normalize(pretty_version, None).unwrap(),
            pretty_version.clone(),
        );

        if let Some(branch_alias) = branch_alias {
            let mut alias_map: IndexMap<BString, PhpMixed> = IndexMap::new();
            alias_map.insert(
                pretty_version.as_bstr().into(),
                PhpMixed::string(branch_alias.to_php_string()),
            );
            let mut extra: IndexMap<BString, PhpMixed> = IndexMap::new();
            extra.insert("branch-alias".into(), PhpMixed::Array(alias_map));
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
