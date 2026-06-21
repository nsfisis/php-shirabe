//! ref: composer/tests/Composer/Test/DependencyResolver/DefaultPolicyTest.php

use indexmap::IndexMap;
use shirabe::dependency_resolver::default_policy::DefaultPolicy;
use shirabe::repository::array_repository::ArrayRepository;
use shirabe::repository::lock_array_repository::LockArrayRepository;
use shirabe::repository::repository_set::RepositorySet;
use shirabe::util::platform::Platform;

#[allow(dead_code)]
struct Fixtures {
    repository_set: RepositorySet,
    repo: ArrayRepository,
    repo_locked: LockArrayRepository,
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
    let repo_locked = LockArrayRepository::new(vec![]).unwrap();

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

// These build a Pool from packages and exercise DefaultPolicy::selectPreferredPackages.
// Constructing the packages/constraints parses versions through a look-around regex the
// regex crate cannot compile, and the setup mirrors the solver fixtures.
#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_select_single() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_select_newest() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_select_newest_picks_latest() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_select_newest_picks_latest_stable_with_prefer_stable() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_select_lowest_with_prefer_dev_over_prerelease() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_select_lowest_prefers_prerelease_over_dev() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_select_lowest_with_prefer_stable_still_prefers_stable() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_select_newest_with_dev_picks_non_dev() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_select_newest_with_preferred_version_picks_preferred_version_if_available() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_select_newest_with_preferred_version_picks_newest_otherwise() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_select_newest_with_preferred_version_picks_lowest_if_prefer_lowest() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_repository_ordering_affects_priority() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_select_local_repos_first() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_select_all_providers() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_prefer_non_replacing_from_same_repo() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_prefer_replacing_package_from_same_vendor() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (DefaultPolicy over a Pool; constraint parsing uses a look-around regex)"]
fn test_select_lowest() {
    let _tear_down = TearDown;
    let _fixtures = set_up();
    todo!()
}
