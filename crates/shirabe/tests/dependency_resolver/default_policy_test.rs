//! ref: composer/tests/Composer/Test/DependencyResolver/DefaultPolicyTest.php

use indexmap::IndexMap;
use shirabe::dependency_resolver::PolicyInterface;
use shirabe::dependency_resolver::default_policy::DefaultPolicy;
use shirabe::package::Link;
use shirabe::package::handle::{CompleteAliasPackageHandle, CompletePackageHandle};
use shirabe::repository::array_repository::ArrayRepository;
use shirabe::repository::handle::{LockArrayRepositoryHandle, RepositoryInterfaceHandle};
use shirabe::repository::lock_array_repository::LockArrayRepository;
use shirabe::repository::repository_set::RepositorySet;
use shirabe::util::platform::Platform;
use shirabe_semver::constraint::{AnyConstraint, SimpleConstraint};

use crate::test_case::get_package;

#[allow(dead_code)]
struct Fixtures {
    repository_set: RepositorySet,
    repo: ArrayRepository,
    repo_locked: LockArrayRepositoryHandle,
    policy: DefaultPolicy,
}

fn set_up() -> Fixtures {
    let repository_set = RepositorySet::new(
        "dev",
        IndexMap::new(),
        vec![],
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );
    let repo = ArrayRepository::new(vec![]).unwrap();
    let repo_locked = LockArrayRepositoryHandle::new(LockArrayRepository::new(vec![]).unwrap());

    let policy = DefaultPolicy::new(false, false, None);

    Fixtures {
        repository_set,
        repo,
        repo_locked,
        policy,
    }
}

fn tear_down() {
    Platform::clear_env("COMPOSER_PREFER_DEV_OVER_PRERELEASE");
}

struct TearDown;
impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

#[test]
fn test_select_single() {
    let _tear_down = TearDown;
    let mut fixtures = set_up();

    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    fixtures
        .repository_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();

    let pool = fixtures
        .repository_set
        .create_pool_for_package("A", Some(fixtures.repo_locked.clone()))
        .unwrap();

    let literals = vec![package_a.get_id()];
    let expected = vec![package_a.get_id()];

    let selected = fixtures
        .policy
        .select_preferred_packages(&pool, literals, None);

    assert_eq!(expected, selected);
}

#[test]
fn test_select_newest() {
    let _tear_down = TearDown;
    let mut fixtures = set_up();

    let package_a1 = get_package("A", "1.0");
    let package_a2 = get_package("A", "2.0");
    fixtures.repo.add_package(package_a1.clone()).unwrap();
    fixtures.repo.add_package(package_a2.clone()).unwrap();
    fixtures
        .repository_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();

    let pool = fixtures
        .repository_set
        .create_pool_for_package("A", Some(fixtures.repo_locked.clone()))
        .unwrap();

    let literals = vec![package_a1.get_id(), package_a2.get_id()];
    let expected = vec![package_a2.get_id()];

    let selected = fixtures
        .policy
        .select_preferred_packages(&pool, literals, None);

    assert_eq!(expected, selected);
}

#[test]
fn test_select_newest_picks_latest() {
    let _tear_down = TearDown;
    let mut fixtures = set_up();

    let package_a1 = get_package("A", "1.0.0");
    let package_a2 = get_package("A", "1.0.1-alpha");
    fixtures.repo.add_package(package_a1.clone()).unwrap();
    fixtures.repo.add_package(package_a2.clone()).unwrap();
    fixtures
        .repository_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();

    let pool = fixtures
        .repository_set
        .create_pool_for_package("A", Some(fixtures.repo_locked.clone()))
        .unwrap();

    let literals = vec![package_a1.get_id(), package_a2.get_id()];
    let expected = vec![package_a2.get_id()];

    let selected = fixtures
        .policy
        .select_preferred_packages(&pool, literals, None);

    assert_eq!(expected, selected);
}

#[test]
fn test_select_newest_picks_latest_stable_with_prefer_stable() {
    let _tear_down = TearDown;
    let mut fixtures = set_up();

    let package_a1 = get_package("A", "1.0.0");
    let package_a2 = get_package("A", "1.0.1-alpha");
    fixtures.repo.add_package(package_a1.clone()).unwrap();
    fixtures.repo.add_package(package_a2.clone()).unwrap();
    fixtures
        .repository_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();

    let pool = fixtures
        .repository_set
        .create_pool_for_package("A", Some(fixtures.repo_locked.clone()))
        .unwrap();

    let literals = vec![package_a1.get_id(), package_a2.get_id()];
    let expected = vec![package_a1.get_id()];

    let policy = DefaultPolicy::new(true, false, None);
    let selected = policy.select_preferred_packages(&pool, literals, None);

    assert_eq!(expected, selected);
}

#[ignore]
#[test]
fn test_select_lowest_with_prefer_dev_over_prerelease() {
    let _tear_down = TearDown;

    for stability in ["alpha1", "beta1", "RC1"] {
        let mut fixtures = set_up();

        Platform::put_env("COMPOSER_PREFER_DEV_OVER_PRERELEASE", "1");
        let dev_package = get_package("A", "dev-master");
        let prerelease_package = get_package("A", &format!("1.0.0-{}", stability));
        fixtures.repo.add_package(dev_package.clone()).unwrap();
        fixtures
            .repo
            .add_package(prerelease_package.clone())
            .unwrap();
        fixtures
            .repository_set
            .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
            .unwrap();

        let pool = fixtures
            .repository_set
            .create_pool_for_package("A", Some(fixtures.repo_locked.clone()))
            .unwrap();

        let literals = vec![dev_package.get_id(), prerelease_package.get_id()];
        let expected = vec![dev_package.get_id()];

        let policy = DefaultPolicy::new(true, true, None);
        let selected = policy.select_preferred_packages(&pool, literals, None);

        assert_eq!(expected, selected);
    }
}

#[test]
fn test_select_lowest_prefers_prerelease_over_dev() {
    let _tear_down = TearDown;

    for stability in ["alpha1", "beta1", "RC1"] {
        let mut fixtures = set_up();

        let dev_package = get_package("A", "dev-master");
        let prerelease_package = get_package("A", &format!("1.0.0-{}", stability));
        fixtures.repo.add_package(dev_package.clone()).unwrap();
        fixtures
            .repo
            .add_package(prerelease_package.clone())
            .unwrap();
        fixtures
            .repository_set
            .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
            .unwrap();

        let pool = fixtures
            .repository_set
            .create_pool_for_package("A", Some(fixtures.repo_locked.clone()))
            .unwrap();

        let literals = vec![prerelease_package.get_id(), dev_package.get_id()];
        let expected = vec![prerelease_package.get_id()];

        let policy = DefaultPolicy::new(true, true, None);
        let selected = policy.select_preferred_packages(&pool, literals, None);

        assert_eq!(expected, selected);
    }
}

#[test]
fn test_select_lowest_with_prefer_stable_still_prefers_stable() {
    let _tear_down = TearDown;
    let mut fixtures = set_up();

    Platform::put_env("COMPOSER_PREFER_DEV_OVER_PRERELEASE", "1");
    let stable_package = get_package("A", "1.0.0");
    let dev_package = get_package("A", "dev-master");
    fixtures.repo.add_package(stable_package.clone()).unwrap();
    fixtures.repo.add_package(dev_package.clone()).unwrap();
    fixtures
        .repository_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();

    let pool = fixtures
        .repository_set
        .create_pool_for_package("A", Some(fixtures.repo_locked.clone()))
        .unwrap();

    let literals = vec![stable_package.get_id(), dev_package.get_id()];
    let expected = vec![stable_package.get_id()];

    let policy = DefaultPolicy::new(true, true, None);
    let selected = policy.select_preferred_packages(&pool, literals, None);

    assert_eq!(expected, selected);
}

#[test]
fn test_select_newest_with_dev_picks_non_dev() {
    let _tear_down = TearDown;
    let mut fixtures = set_up();

    let package_a1 = get_package("A", "dev-foo");
    let package_a2 = get_package("A", "1.0.0");
    fixtures.repo.add_package(package_a1.clone()).unwrap();
    fixtures.repo.add_package(package_a2.clone()).unwrap();
    fixtures
        .repository_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();

    let pool = fixtures
        .repository_set
        .create_pool_for_package("A", Some(fixtures.repo_locked.clone()))
        .unwrap();

    let literals = vec![package_a1.get_id(), package_a2.get_id()];
    let expected = vec![package_a2.get_id()];

    let selected = fixtures
        .policy
        .select_preferred_packages(&pool, literals, None);

    assert_eq!(expected, selected);
}

#[test]
fn test_select_newest_with_preferred_version_picks_preferred_version_if_available() {
    let _tear_down = TearDown;
    let mut fixtures = set_up();

    let package_a1 = get_package("A", "1.0.0");
    let package_a2 = get_package("A", "1.1.0");
    let package_a2b = get_package("A", "1.1.0");
    let package_a3 = get_package("A", "1.2.0");
    fixtures.repo.add_package(package_a1.clone()).unwrap();
    fixtures.repo.add_package(package_a2.clone()).unwrap();
    fixtures.repo.add_package(package_a2b.clone()).unwrap();
    fixtures.repo.add_package(package_a3.clone()).unwrap();
    fixtures
        .repository_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();

    let pool = fixtures
        .repository_set
        .create_pool_for_package("A", Some(fixtures.repo_locked.clone()))
        .unwrap();

    let literals = vec![
        package_a1.get_id(),
        package_a2.get_id(),
        package_a2b.get_id(),
        package_a3.get_id(),
    ];
    let expected = vec![package_a2.get_id(), package_a2b.get_id()];

    let mut preferred = IndexMap::new();
    preferred.insert("a".to_string(), "1.1.0.0".to_string());
    let policy = DefaultPolicy::new(false, false, Some(preferred));
    let selected = policy.select_preferred_packages(&pool, literals, None);

    assert_eq!(expected, selected);
}

#[test]
fn test_select_newest_with_preferred_version_picks_newest_otherwise() {
    let _tear_down = TearDown;
    let mut fixtures = set_up();

    let package_a1 = get_package("A", "1.0.0");
    let package_a2 = get_package("A", "1.2.0");
    fixtures.repo.add_package(package_a1.clone()).unwrap();
    fixtures.repo.add_package(package_a2.clone()).unwrap();
    fixtures
        .repository_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();

    let pool = fixtures
        .repository_set
        .create_pool_for_package("A", Some(fixtures.repo_locked.clone()))
        .unwrap();

    let literals = vec![package_a1.get_id(), package_a2.get_id()];
    let expected = vec![package_a2.get_id()];

    let mut preferred = IndexMap::new();
    preferred.insert("a".to_string(), "1.1.0.0".to_string());
    let policy = DefaultPolicy::new(false, false, Some(preferred));
    let selected = policy.select_preferred_packages(&pool, literals, None);

    assert_eq!(expected, selected);
}

#[test]
fn test_select_newest_with_preferred_version_picks_lowest_if_prefer_lowest() {
    let _tear_down = TearDown;
    let mut fixtures = set_up();

    let package_a1 = get_package("A", "1.0.0");
    let package_a2 = get_package("A", "1.2.0");
    fixtures.repo.add_package(package_a1.clone()).unwrap();
    fixtures.repo.add_package(package_a2.clone()).unwrap();
    fixtures
        .repository_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();

    let pool = fixtures
        .repository_set
        .create_pool_for_package("A", Some(fixtures.repo_locked.clone()))
        .unwrap();

    let literals = vec![package_a1.get_id(), package_a2.get_id()];
    let expected = vec![package_a1.get_id()];

    let mut preferred = IndexMap::new();
    preferred.insert("a".to_string(), "1.1.0.0".to_string());
    let policy = DefaultPolicy::new(false, true, Some(preferred));
    let selected = policy.select_preferred_packages(&pool, literals, None);

    assert_eq!(expected, selected);
}

#[test]
fn test_repository_ordering_affects_priority() {
    let _tear_down = TearDown;
    let mut fixtures = set_up();

    let repo1 = ArrayRepository::new(vec![]).unwrap();
    let repo2 = ArrayRepository::new(vec![]).unwrap();

    let package1 = get_package("A", "1.0");
    let package2 = get_package("A", "1.1");
    let package3 = get_package("A", "1.1");
    let package4 = get_package("A", "1.2");
    repo1.add_package(package1.clone()).unwrap();
    repo1.add_package(package2.clone()).unwrap();
    repo2.add_package(package3.clone()).unwrap();
    repo2.add_package(package4.clone()).unwrap();

    let repo1_handle = RepositoryInterfaceHandle::new(repo1);
    let repo2_handle = RepositoryInterfaceHandle::new(repo2);

    fixtures
        .repository_set
        .add_repository(repo1_handle.clone())
        .unwrap();
    fixtures
        .repository_set
        .add_repository(repo2_handle.clone())
        .unwrap();

    let pool = fixtures
        .repository_set
        .create_pool_for_package("A", Some(fixtures.repo_locked.clone()))
        .unwrap();

    let literals = vec![
        package1.get_id(),
        package2.get_id(),
        package3.get_id(),
        package4.get_id(),
    ];
    let expected = vec![package2.get_id()];
    let selected = fixtures
        .policy
        .select_preferred_packages(&pool, literals.clone(), None);

    assert_eq!(expected, selected);

    let mut repository_set = RepositorySet::new(
        "dev",
        IndexMap::new(),
        vec![],
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );
    repository_set.add_repository(repo2_handle).unwrap();
    repository_set.add_repository(repo1_handle).unwrap();

    let pool = repository_set
        .create_pool_for_package("A", Some(fixtures.repo_locked.clone()))
        .unwrap();

    let expected = vec![package4.get_id()];
    let selected = fixtures
        .policy
        .select_preferred_packages(&pool, literals, None);

    assert_eq!(expected, selected);
}

#[ignore]
#[test]
fn test_select_local_repos_first() {
    let _tear_down = TearDown;
    let mut fixtures = set_up();

    let repo_important = ArrayRepository::new(vec![]).unwrap();

    let package_a = get_package("A", "dev-master");
    let package_a_alias = CompleteAliasPackageHandle::new(
        CompletePackageHandle::from_rc_unchecked(package_a.as_rc().clone()),
        "2.1.9999999.9999999-dev".to_string(),
        "2.1.x-dev".to_string(),
    );
    let package_a_important = get_package("A", "dev-feature-a");
    let package_a_alias_important = CompleteAliasPackageHandle::new(
        CompletePackageHandle::from_rc_unchecked(package_a_important.as_rc().clone()),
        "2.1.9999999.9999999-dev".to_string(),
        "2.1.x-dev".to_string(),
    );
    let package_a2_important = get_package("A", "dev-master");
    let package_a2_alias_important = CompleteAliasPackageHandle::new(
        CompletePackageHandle::from_rc_unchecked(package_a2_important.as_rc().clone()),
        "2.1.9999999.9999999-dev".to_string(),
        "2.1.x-dev".to_string(),
    );
    package_a_alias_important.set_root_package_alias(true);

    fixtures.repo.add_package(package_a).unwrap();
    fixtures
        .repo
        .add_package(package_a_alias.clone().into())
        .unwrap();
    repo_important.add_package(package_a_important).unwrap();
    repo_important
        .add_package(package_a_alias_important.clone().into())
        .unwrap();
    repo_important.add_package(package_a2_important).unwrap();
    repo_important
        .add_package(package_a2_alias_important.into())
        .unwrap();

    fixtures
        .repository_set
        .add_repository(RepositoryInterfaceHandle::new(repo_important))
        .unwrap();
    fixtures
        .repository_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();
    fixtures
        .repository_set
        .add_repository(fixtures.repo_locked.clone().into())
        .unwrap();

    let mut pool = fixtures
        .repository_set
        .create_pool_for_package("A", Some(fixtures.repo_locked.clone()))
        .unwrap();

    let constraint = AnyConstraint::Simple(SimpleConstraint::new(
        "=".to_string(),
        "2.1.9999999.9999999-dev".to_string(),
        None,
    ));
    let packages = pool.what_provides("a", Some(&constraint));
    assert!(!packages.is_empty());
    let mut literals = vec![];
    for package in &packages {
        literals.push(package.get_id());
    }

    let expected = vec![package_a_alias_important.get_id()];

    let selected = fixtures
        .policy
        .select_preferred_packages(&pool, literals, None);

    assert_eq!(expected, selected);
}

/// PHP `new Link($source, $target, $constraint, $type)`: prettyConstraint defaults to
/// `(string) $constraint`.
fn link(source: &str, target: &str, constraint: AnyConstraint, r#type: &str) -> Link {
    let pretty = constraint.get_pretty_string();
    Link::new(
        source.to_string(),
        target.to_string(),
        constraint,
        Some(r#type.to_string()),
        pretty,
    )
}

fn as_complete(
    package: &shirabe::package::handle::PackageInterfaceHandle,
) -> CompletePackageHandle {
    CompletePackageHandle::from_rc_unchecked(package.as_rc().clone())
}

fn constraint(operator: &str, version: &str) -> AnyConstraint {
    SimpleConstraint::new(operator.to_string(), version.to_string(), None).into()
}

#[test]
fn test_select_all_providers() {
    let _tear_down = TearDown;
    let mut fixtures = set_up();

    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "2.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();

    let mut provides_a: IndexMap<String, Link> = IndexMap::new();
    provides_a.insert(
        "x".to_string(),
        link("A", "X", constraint("==", "1.0"), Link::TYPE_PROVIDE),
    );
    as_complete(&package_a).__set_provides(provides_a);
    let mut provides_b: IndexMap<String, Link> = IndexMap::new();
    provides_b.insert(
        "x".to_string(),
        link("B", "X", constraint("==", "1.0"), Link::TYPE_PROVIDE),
    );
    as_complete(&package_b).__set_provides(provides_b);

    fixtures
        .repository_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();

    let pool = fixtures
        .repository_set
        .create_pool_for_packages(
            vec!["A".to_string(), "B".to_string()],
            Some(fixtures.repo_locked.clone()),
        )
        .unwrap();

    let literals = vec![package_a.get_id(), package_b.get_id()];
    let expected = literals.clone();

    let selected = fixtures
        .policy
        .select_preferred_packages(&pool, literals, None);

    assert_eq!(expected, selected);
}

#[test]
fn test_prefer_non_replacing_from_same_repo() {
    let _tear_down = TearDown;
    let mut fixtures = set_up();

    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "2.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();

    let mut replaces_b: IndexMap<String, Link> = IndexMap::new();
    replaces_b.insert(
        "a".to_string(),
        link("B", "A", constraint("==", "1.0"), Link::TYPE_REPLACE),
    );
    as_complete(&package_b).__set_replaces(replaces_b);

    fixtures
        .repository_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();

    let pool = fixtures
        .repository_set
        .create_pool_for_packages(
            vec!["A".to_string(), "B".to_string()],
            Some(fixtures.repo_locked.clone()),
        )
        .unwrap();

    let literals = vec![package_a.get_id(), package_b.get_id()];
    let expected = literals.clone();

    let selected = fixtures
        .policy
        .select_preferred_packages(&pool, literals, None);

    assert_eq!(expected, selected);
}

#[test]
fn test_prefer_replacing_package_from_same_vendor() {
    let _tear_down = TearDown;
    let mut fixtures = set_up();

    // test with default order
    let package_b = get_package("vendor-b/replacer", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();
    let package_a = get_package("vendor-a/replacer", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();

    let mut replaces_a: IndexMap<String, Link> = IndexMap::new();
    replaces_a.insert(
        "vendor-a/package".to_string(),
        link(
            "vendor-a/replacer",
            "vendor-a/package",
            constraint("==", "1.0"),
            Link::TYPE_REPLACE,
        ),
    );
    as_complete(&package_a).__set_replaces(replaces_a);
    let mut replaces_b: IndexMap<String, Link> = IndexMap::new();
    replaces_b.insert(
        "vendor-a/package".to_string(),
        link(
            "vendor-b/replacer",
            "vendor-a/package",
            constraint("==", "1.0"),
            Link::TYPE_REPLACE,
        ),
    );
    as_complete(&package_b).__set_replaces(replaces_b);

    fixtures
        .repository_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();

    let pool = fixtures
        .repository_set
        .create_pool_for_packages(
            vec![
                "vendor-a/replacer".to_string(),
                "vendor-b/replacer".to_string(),
            ],
            Some(fixtures.repo_locked.clone()),
        )
        .unwrap();

    let literals = vec![package_a.get_id(), package_b.get_id()];
    let expected = literals.clone();

    let selected = fixtures.policy.select_preferred_packages(
        &pool,
        literals,
        Some("vendor-a/package".to_string()),
    );
    assert_eq!(expected, selected);

    // test with reversed order in repo
    let repo = ArrayRepository::new(vec![]).unwrap();
    let package_a = CompletePackageHandle::dup(&as_complete(&package_a));
    repo.add_package(package_a.clone().into()).unwrap();
    let package_b = CompletePackageHandle::dup(&as_complete(&package_b));
    repo.add_package(package_b.clone().into()).unwrap();

    let mut repository_set = RepositorySet::new(
        "dev",
        IndexMap::new(),
        vec![],
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );
    repository_set
        .add_repository(RepositoryInterfaceHandle::new(repo))
        .unwrap();

    let pool = fixtures
        .repository_set
        .create_pool_for_packages(
            vec![
                "vendor-a/replacer".to_string(),
                "vendor-b/replacer".to_string(),
            ],
            Some(fixtures.repo_locked.clone()),
        )
        .unwrap();

    let literals = vec![package_a.get_id(), package_b.get_id()];
    let expected = literals.clone();

    let selected = fixtures.policy.select_preferred_packages(
        &pool,
        literals,
        Some("vendor-a/package".to_string()),
    );
    assert_eq!(expected, selected);
}

#[test]
fn test_select_lowest() {
    let _tear_down = TearDown;
    let mut fixtures = set_up();

    let policy = DefaultPolicy::new(false, true, None);

    let package_a1 = get_package("A", "1.0");
    let package_a2 = get_package("A", "2.0");
    fixtures.repo.add_package(package_a1.clone()).unwrap();
    fixtures.repo.add_package(package_a2.clone()).unwrap();
    fixtures
        .repository_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();

    let pool = fixtures
        .repository_set
        .create_pool_for_package("A", Some(fixtures.repo_locked.clone()))
        .unwrap();

    let literals = vec![package_a1.get_id(), package_a2.get_id()];
    let expected = vec![package_a1.get_id()];

    let selected = policy.select_preferred_packages(&pool, literals, None);

    assert_eq!(expected, selected);
}
