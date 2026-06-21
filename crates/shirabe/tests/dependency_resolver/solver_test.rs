//! ref: composer/tests/Composer/Test/DependencyResolver/SolverTest.php

use indexmap::IndexMap;
use shirabe::dependency_resolver::default_policy::DefaultPolicy;
use shirabe::dependency_resolver::request::Request;
use shirabe::repository::array_repository::ArrayRepository;
use shirabe::repository::handle::LockArrayRepositoryHandle;
use shirabe::repository::lock_array_repository::LockArrayRepository;
use shirabe::repository::repository_set::RepositorySet;

#[allow(dead_code)]
struct Fixtures {
    repo_set: RepositorySet,
    repo: ArrayRepository,
    repo_locked: LockArrayRepositoryHandle,
    request: Request,
    policy: DefaultPolicy,
}

fn set_up() -> Fixtures {
    let repo_set = RepositorySet::new(
        "stable",
        IndexMap::new(),
        vec![],
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );
    let repo = ArrayRepository::new(vec![]).unwrap();
    let repo_locked = LockArrayRepositoryHandle::new(LockArrayRepository::new(vec![]).unwrap());

    let request = Request::new(Some(repo_locked.clone()));
    let policy = DefaultPolicy::new(false, false, None);

    Fixtures {
        repo_set,
        repo,
        repo_locked,
        request,
        policy,
    }
}

// These run the dependency Solver over packages/requests built from version constraints,
// whose parsing goes through a look-around regex the regex crate cannot compile; the setup
// also mirrors the larger solver fixtures.
#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_install_single() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_remove_if_not_requested() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_install_non_existing_package_fails() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_install_same_package_from_different_repositories() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_install_with_deps() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_install_honours_not_equal_operator() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_install_with_deps_in_order() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_multi_package_name_version_resolution_depends_on_require_order() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_multi_package_name_version_resolution_is_independent_of_require_order_if_ordered_descending_by_requirement()
 {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_fix_locked() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_fix_locked_with_alternative() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_update_does_only_update() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_update_single() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_update_all() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_update_current() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_update_only_updates_selected_package() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_update_constrained() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_update_fully_constrained() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_update_fully_constrained_prunes_installed_packages() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_all_jobs() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_three_alternative_require_and_conflict() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_solver_obsolete() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_install_one_of_two_alternatives() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_install_provider() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_skip_replacer_of_existing_package() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_no_install_replacer_of_missing_package() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_skip_replaced_package_if_replacer_is_selected() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_pick_older_if_newer_conflicts() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_install_circular_require() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_install_alternative_with_circular_require() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_use_replacer_if_necessary() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_issue265() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_conflict_result_empty() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_unsatisfiable_requires() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_require_mismatch_exception() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_learn_literals_with_sorted_rule_literals() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_install_recursive_alias_dependencies() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_install_dev_alias() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_install_root_aliases_if_alias_of_is_installed() {
    let _fixtures = set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
fn test_learn_positive_literal() {
    let _fixtures = set_up();
    todo!()
}
