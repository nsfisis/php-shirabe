//! ref: composer/tests/Composer/Test/DependencyResolver/SolverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::dependency_resolver::PolicyInterface;
use shirabe::dependency_resolver::default_policy::DefaultPolicy;
use shirabe::dependency_resolver::request::Request;
use shirabe::io::io_interface::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe::repository::array_repository::ArrayRepository;
use shirabe::repository::handle::{LockArrayRepositoryHandle, RepositoryInterfaceHandle};
use shirabe::repository::lock_array_repository::LockArrayRepository;
use shirabe::repository::repository_set::RepositorySet;

use crate::test_case::{get_alias_package, get_package, get_version_constraint};

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

/// One expected solver job. Mirrors the PHP `['job' => ..., 'package'|'from'|'to' => ...]` rows.
enum ExpectedJob {
    Single {
        job: &'static str,
        package: PackageInterfaceHandle,
    },
    Update {
        from: PackageInterfaceHandle,
        to: PackageInterfaceHandle,
    },
}

/// ref: SolverTest::checkSolverResult (with reposComplete + createSolver folded in).
fn check_solver_result(
    mut repo_set: RepositorySet,
    repo: ArrayRepository,
    repo_locked: LockArrayRepositoryHandle,
    mut request: Request,
    expected: Vec<ExpectedJob>,
) {
    // reposComplete()
    repo_set
        .add_repository(RepositoryInterfaceHandle::new(repo))
        .unwrap();
    repo_set.add_repository(repo_locked.into()).unwrap();

    // createSolver()
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let pool = repo_set
        .create_pool(&mut request, io.clone(), None, None, vec![], None, None)
        .unwrap();
    let policy: Rc<dyn PolicyInterface> = Rc::new(DefaultPolicy::new(false, false, None));
    let mut solver =
        shirabe::dependency_resolver::solver::Solver::new(policy, Rc::new(RefCell::new(pool)), io);

    let transaction = solver.solve(&request, None).unwrap();

    // Build readable (unique-name) and identity (ptr) representations of the result and
    // the expectation, mirroring the dual assertEquals in the PHP helper.
    let mut result_readable: Vec<(String, String)> = Vec::new();
    let mut result_ids: Vec<(String, Vec<usize>)> = Vec::new();
    for operation in transaction.get_operations() {
        if let Some(update) = operation.as_update_operation() {
            let from = update.get_initial_package();
            let to = update.get_target_package();
            result_readable.push((
                "update".to_string(),
                format!("{} => {}", from.get_unique_name(), to.get_unique_name()),
            ));
            result_ids.push(("update".to_string(), vec![from.ptr_id(), to.ptr_id()]));
        } else {
            let op_type = operation.get_operation_type();
            let job = match op_type.as_str() {
                "markAliasInstalled" => "markAliasInstalled",
                "markAliasUninstalled" => "markAliasUninstalled",
                "uninstall" => "remove",
                "install" => "install",
                other => panic!("Unexpected operation: {}", other),
            };
            let package = operation.get_package();
            result_readable.push((job.to_string(), package.get_unique_name()));
            result_ids.push((job.to_string(), vec![package.ptr_id()]));
        }
    }

    let mut expected_readable: Vec<(String, String)> = Vec::new();
    let mut expected_ids: Vec<(String, Vec<usize>)> = Vec::new();
    for job in &expected {
        match job {
            ExpectedJob::Single { job, package } => {
                expected_readable.push((job.to_string(), package.get_unique_name()));
                expected_ids.push((job.to_string(), vec![package.ptr_id()]));
            }
            ExpectedJob::Update { from, to } => {
                expected_readable.push((
                    "update".to_string(),
                    format!("{} => {}", from.get_unique_name(), to.get_unique_name()),
                ));
                expected_ids.push(("update".to_string(), vec![from.ptr_id(), to.ptr_id()]));
            }
        }
    }

    assert_eq!(expected_readable, result_readable);
    assert_eq!(expected_ids, result_ids);
}

#[ignore]
#[test]
fn test_solver_install_single() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![ExpectedJob::Single {
            job: "install",
            package: package_a,
        }],
    );
}

#[ignore]
#[test]
fn test_solver_remove_if_not_requested() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo_locked.add_package(package_a.clone()).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        fixtures.request,
        vec![ExpectedJob::Single {
            job: "remove",
            package: package_a,
        }],
    );
}

#[ignore = "solve() error path discards SolverProblemsException (returns placeholder anyhow error); getProblems/getCode/getPrettyString not retrievable"]
#[test]
fn test_install_non_existing_package_fails() {
    let _fixtures = set_up();
    todo!()
}

#[ignore]
#[test]
fn test_solver_install_same_package_from_different_repositories() {
    let fixtures = set_up();
    let mut repo_set = fixtures.repo_set;

    let repo1 = ArrayRepository::new(vec![]).unwrap();
    let repo2 = ArrayRepository::new(vec![]).unwrap();

    let foo1 = get_package("foo", "1");
    let foo2 = get_package("foo", "1");
    repo1.add_package(foo1.clone()).unwrap();
    repo2.add_package(foo2.clone()).unwrap();

    repo_set
        .add_repository(RepositoryInterfaceHandle::new(repo1))
        .unwrap();
    repo_set
        .add_repository(RepositoryInterfaceHandle::new(repo2))
        .unwrap();

    let mut request = fixtures.request;
    request.require_name("foo", None).unwrap();

    // The two repos are already added here; the helper adds the (empty) default repos too.
    check_solver_result(
        repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![ExpectedJob::Single {
            job: "install",
            package: foo1,
        }],
    );
}

#[ignore = "setRequires not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_solver_install_with_deps() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_solver_install_honours_not_equal_operator() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_solver_install_with_deps_in_order() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_solver_multi_package_name_version_resolution_depends_on_require_order() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_solver_multi_package_name_version_resolution_is_independent_of_require_order_if_ordered_descending_by_requirement()
 {
    let _fixtures = set_up();
    todo!()
}

#[ignore]
#[test]
fn test_solver_fix_locked() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo_locked.add_package(package_a.clone()).unwrap();

    let mut request = fixtures.request;
    request.fix_package(package_a.clone());

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![],
    );
}

#[ignore]
#[test]
fn test_solver_fix_locked_with_alternative() {
    let fixtures = set_up();
    fixtures.repo.add_package(get_package("A", "1.0")).unwrap();
    let package_a = get_package("A", "1.0");
    fixtures.repo_locked.add_package(package_a.clone()).unwrap();

    let mut request = fixtures.request;
    request.fix_package(package_a.clone());

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![],
    );
}

#[ignore = "setRequires not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_solver_update_does_only_update() {
    let _fixtures = set_up();
    todo!()
}

#[ignore]
#[test]
fn test_solver_update_single() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo_locked.add_package(package_a.clone()).unwrap();
    let new_package_a = get_package("A", "1.1");
    fixtures.repo.add_package(new_package_a.clone()).unwrap();

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![ExpectedJob::Update {
            from: package_a,
            to: new_package_a,
        }],
    );
}

#[ignore = "setRequires not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_solver_update_all() {
    let _fixtures = set_up();
    todo!()
}

#[ignore]
#[test]
fn test_solver_update_current() {
    let fixtures = set_up();
    fixtures
        .repo_locked
        .add_package(get_package("A", "1.0"))
        .unwrap();
    fixtures.repo.add_package(get_package("A", "1.0")).unwrap();

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![],
    );
}

#[ignore]
#[test]
fn test_solver_update_only_updates_selected_package() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo_locked.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo_locked.add_package(package_b.clone()).unwrap();
    let package_a_newer = get_package("A", "1.1");
    fixtures.repo.add_package(package_a_newer.clone()).unwrap();
    let package_b_newer = get_package("B", "1.1");
    fixtures.repo.add_package(package_b_newer.clone()).unwrap();

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();
    request.fix_package(package_b.clone());

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![ExpectedJob::Update {
            from: package_a,
            to: package_a_newer,
        }],
    );
}

#[ignore]
#[test]
fn test_solver_update_constrained() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo_locked.add_package(package_a.clone()).unwrap();
    let new_package_a = get_package("A", "1.2");
    fixtures.repo.add_package(new_package_a.clone()).unwrap();
    fixtures.repo.add_package(get_package("A", "2.0")).unwrap();

    let mut request = fixtures.request;
    request
        .require_name("A", Some(get_version_constraint("<", "2.0.0.0")))
        .unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![ExpectedJob::Update {
            from: package_a,
            to: new_package_a,
        }],
    );
}

#[ignore]
#[test]
fn test_solver_update_fully_constrained() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo_locked.add_package(package_a.clone()).unwrap();
    let new_package_a = get_package("A", "1.2");
    fixtures.repo.add_package(new_package_a.clone()).unwrap();
    fixtures.repo.add_package(get_package("A", "2.0")).unwrap();

    let mut request = fixtures.request;
    request
        .require_name("A", Some(get_version_constraint("<", "2.0.0.0")))
        .unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![ExpectedJob::Update {
            from: package_a,
            to: new_package_a,
        }],
    );
}

#[ignore]
#[test]
fn test_solver_update_fully_constrained_prunes_installed_packages() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo_locked.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo_locked.add_package(package_b.clone()).unwrap();
    let new_package_a = get_package("A", "1.2");
    fixtures.repo.add_package(new_package_a.clone()).unwrap();
    fixtures.repo.add_package(get_package("A", "2.0")).unwrap();

    let mut request = fixtures.request;
    request
        .require_name("A", Some(get_version_constraint("<", "2.0.0.0")))
        .unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![
            ExpectedJob::Single {
                job: "remove",
                package: package_b,
            },
            ExpectedJob::Update {
                from: package_a,
                to: new_package_a,
            },
        ],
    );
}

#[ignore = "setRequires not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_solver_all_jobs() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires/setConflicts not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_solver_three_alternative_require_and_conflict() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setReplaces not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_solver_obsolete() {
    let _fixtures = set_up();
    todo!()
}

#[ignore]
#[test]
fn test_install_one_of_two_alternatives() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_b = get_package("A", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![ExpectedJob::Single {
            job: "install",
            package: package_a,
        }],
    );
}

#[ignore = "setRequires/setProvides not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_install_provider() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires/setReplaces not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_skip_replacer_of_existing_package() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires/setReplaces not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_no_install_replacer_of_missing_package() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires/setReplaces not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_skip_replaced_package_if_replacer_is_selected() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires/setReplaces not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_pick_older_if_newer_conflicts() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_install_circular_require() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires/setProvides not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_install_alternative_with_circular_require() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires/setReplaces not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_use_replacer_if_necessary() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires/setReplaces not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_issue265() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setConflicts not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_conflict_result_empty() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires not available on CompletePackageHandle; also asserts SolverProblemsException details which solve() discards"]
#[test]
fn test_unsatisfiable_requires() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires not available on CompletePackageHandle; also asserts SolverProblemsException details which solve() discards"]
#[test]
fn test_require_mismatch_exception() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires/setReplaces not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_learn_literals_with_sorted_rule_literals() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_install_recursive_alias_dependencies() {
    let _fixtures = set_up();
    todo!()
}

#[ignore = "setRequires not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_install_dev_alias() {
    let _fixtures = set_up();
    todo!()
}

#[ignore]
#[test]
fn test_install_root_aliases_if_alias_of_is_installed() {
    let fixtures = set_up();

    // root aliased, required
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_a_alias = get_alias_package(&package_a, "1.1");
    fixtures.repo.add_package(package_a_alias.clone()).unwrap();
    package_a_alias
        .as_alias()
        .unwrap()
        .set_root_package_alias(true);
    // root aliased, not required, should still be installed as it is root alias
    let package_b = get_package("B", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();
    let package_b_alias = get_alias_package(&package_b, "1.1");
    fixtures.repo.add_package(package_b_alias.clone()).unwrap();
    package_b_alias
        .as_alias()
        .unwrap()
        .set_root_package_alias(true);
    // regular alias, not required, alias should not be installed
    let package_c = get_package("C", "1.0");
    fixtures.repo.add_package(package_c.clone()).unwrap();
    let package_c_alias = get_alias_package(&package_c, "1.1");
    fixtures.repo.add_package(package_c_alias.clone()).unwrap();

    let mut request = fixtures.request;
    request
        .require_name("A", Some(get_version_constraint("==", "1.1")))
        .unwrap();
    request
        .require_name("B", Some(get_version_constraint("==", "1.0")))
        .unwrap();
    request
        .require_name("C", Some(get_version_constraint("==", "1.0")))
        .unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![
            ExpectedJob::Single {
                job: "install",
                package: package_a,
            },
            ExpectedJob::Single {
                job: "markAliasInstalled",
                package: package_a_alias,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_b,
            },
            ExpectedJob::Single {
                job: "markAliasInstalled",
                package: package_b_alias,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_c,
            },
            ExpectedJob::Single {
                job: "markAliasInstalled",
                package: package_c_alias,
            },
        ],
    );
}

#[ignore = "setRequires not available on CompletePackageHandle (only RootPackageHandle exposes link setters)"]
#[test]
fn test_learn_positive_literal() {
    let _fixtures = set_up();
    todo!()
}
