//! ref: composer/tests/Composer/Test/DependencyResolver/SolverTest.php

use crate::test_case::{get_alias_package, get_package, get_version_constraint};
use indexmap::IndexMap;
use shirabe::dependency_resolver::PolicyInterface;
use shirabe::dependency_resolver::default_policy::DefaultPolicy;
use shirabe::dependency_resolver::pool::Pool;
use shirabe::dependency_resolver::request::Request;
use shirabe::dependency_resolver::solver_problems_exception::SolverProblemsException;
use shirabe::io::io_interface::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::Link;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe::repository::array_repository::ArrayRepository;
use shirabe::repository::handle::{LockArrayRepositoryHandle, RepositoryInterfaceHandle};
use shirabe::repository::lock_array_repository::LockArrayRepository;
use shirabe::repository::repository_set::RepositorySet;
use shirabe_semver::constraint::{AnyConstraint, MatchAllConstraint, MultiConstraint};

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

/// PHP `new Link($source, $target, $constraint, $type, $prettyConstraint)`.
fn link_pretty(
    source: &str,
    target: &str,
    constraint: AnyConstraint,
    r#type: &str,
    pretty: &str,
) -> Link {
    Link::new(
        source.to_string(),
        target.to_string(),
        constraint,
        Some(r#type.to_string()),
        pretty.to_string(),
    )
}

fn multi(constraints: Vec<AnyConstraint>) -> AnyConstraint {
    MultiConstraint::new(constraints, true, None).into()
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
    request: Request,
    expected: Vec<ExpectedJob>,
) {
    // reposComplete()
    repo_set
        .add_repository(RepositoryInterfaceHandle::new(repo))
        .unwrap();
    repo_set.add_repository(repo_locked.into()).unwrap();

    check_solver_result_repo_set(&mut repo_set, request, expected);
}

/// ref: SolverTest::checkSolverResult, against an already-completed RepositorySet (createSolver).
fn check_solver_result_repo_set(
    repo_set: &mut RepositorySet,
    mut request: Request,
    expected: Vec<ExpectedJob>,
) {
    // createSolver()
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let pool = repo_set
        .create_pool(&mut request, io.clone(), None, None, vec![], None, None)
        .unwrap();
    let policy: std::rc::Rc<dyn PolicyInterface> =
        std::rc::Rc::new(DefaultPolicy::new(false, false, None));
    let mut solver = shirabe::dependency_resolver::solver::Solver::new(
        policy,
        std::rc::Rc::new(std::cell::RefCell::new(pool)),
        io,
    );

    let transaction = solver.solve(&request, None).unwrap().unwrap();

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

/// ref: SolverTest::createSolver + solve, returning the error for expectException-only tests.
fn solve_expecting_error(
    repo_set: RepositorySet,
    repo: ArrayRepository,
    repo_locked: LockArrayRepositoryHandle,
    request: Request,
) {
    solve_expecting_problems(repo_set, repo, repo_locked, request);
}

/// The SolverProblemsException plus the context needed to render its pretty string in assertions.
struct SolveError {
    exception: SolverProblemsException,
    repo_set: RepositorySet,
    request: Request,
    pool: std::rc::Rc<std::cell::RefCell<Pool>>,
}

/// ref: SolverTest::createSolver + solve, returning the caught SolverProblemsException.
fn solve_expecting_problems(
    mut repo_set: RepositorySet,
    repo: ArrayRepository,
    repo_locked: LockArrayRepositoryHandle,
    mut request: Request,
) -> SolveError {
    repo_set
        .add_repository(RepositoryInterfaceHandle::new(repo))
        .unwrap();
    repo_set.add_repository(repo_locked.into()).unwrap();

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let pool = std::rc::Rc::new(std::cell::RefCell::new(
        repo_set
            .create_pool(&mut request, io.clone(), None, None, vec![], None, None)
            .unwrap(),
    ));
    let policy: std::rc::Rc<dyn PolicyInterface> =
        std::rc::Rc::new(DefaultPolicy::new(false, false, None));
    let mut solver = shirabe::dependency_resolver::solver::Solver::new(policy, pool.clone(), io);

    let exception = match solver.solve(&request, None).unwrap() {
        Ok(_) => panic!("Unsolvable conflict did not result in exception."),
        Err(e) => e,
    };

    SolveError {
        exception,
        repo_set,
        request,
        pool,
    }
}

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

#[test]
fn test_install_non_existing_package_fails() {
    let fixtures = set_up();
    fixtures.repo.add_package(get_package("A", "1.0")).unwrap();

    let mut request = fixtures.request;
    request
        .require_name("B", Some(get_version_constraint("==", "1")))
        .unwrap();

    let SolveError {
        exception,
        repo_set,
        request,
        pool,
    } = solve_expecting_problems(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
    );

    let problems = exception.get_problems();
    assert_eq!(problems.len(), 1);
    assert_eq!(exception.get_code(), 2);
    assert_eq!(
        problems[0]
            .get_pretty_string(
                &repo_set,
                &request,
                &mut pool.borrow_mut(),
                false,
                &IndexMap::new(),
                &Vec::new(),
            )
            .unwrap(),
        "\n    - Root composer.json requires b, it could not be found in any version, there may be a typo in the package name."
    );
}

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

#[test]
fn test_solver_install_with_deps() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();
    let new_package_b = get_package("B", "1.1");
    fixtures.repo.add_package(new_package_b.clone()).unwrap();

    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                get_version_constraint("<", "1.1"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![
            ExpectedJob::Single {
                job: "install",
                package: package_b,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_a,
            },
        ],
    );
}

#[test]
fn test_solver_install_honours_not_equal_operator() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();
    let new_package_b11 = get_package("B", "1.1");
    fixtures.repo.add_package(new_package_b11.clone()).unwrap();
    let new_package_b12 = get_package("B", "1.2");
    fixtures.repo.add_package(new_package_b12.clone()).unwrap();
    let new_package_b13 = get_package("B", "1.3");
    fixtures.repo.add_package(new_package_b13.clone()).unwrap();

    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                multi(vec![
                    get_version_constraint("<=", "1.3"),
                    get_version_constraint("<>", "1.3"),
                    get_version_constraint("!=", "1.2"),
                ]),
                Link::TYPE_REQUIRE,
            ),
        )]));

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![
            ExpectedJob::Single {
                job: "install",
                package: new_package_b11,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_a,
            },
        ],
    );
}

#[test]
fn test_solver_install_with_deps_in_order() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();
    let package_c = get_package("C", "1.0");
    fixtures.repo.add_package(package_c.clone()).unwrap();

    package_b
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([
            (
                "a".to_string(),
                link(
                    "B",
                    "A",
                    get_version_constraint(">=", "1.0"),
                    Link::TYPE_REQUIRE,
                ),
            ),
            (
                "c".to_string(),
                link(
                    "B",
                    "C",
                    get_version_constraint(">=", "1.0"),
                    Link::TYPE_REQUIRE,
                ),
            ),
        ]));
    package_c
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "a".to_string(),
            link(
                "C",
                "A",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();
    request.require_name("B", None).unwrap();
    request.require_name("C", None).unwrap();

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
                job: "install",
                package: package_c,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_b,
            },
        ],
    );
}

#[test]
fn test_solver_multi_package_name_version_resolution_depends_on_require_order() {
    let fixtures = set_up();
    let php74 = get_package("ourcustom/PHP", "7.4.23");
    fixtures.repo.add_package(php74.clone()).unwrap();
    let php80 = get_package("ourcustom/PHP", "8.0.10");
    fixtures.repo.add_package(php80.clone()).unwrap();
    let ext_for_php74 = get_package("ourcustom/ext-foobar", "1.0");
    fixtures.repo.add_package(ext_for_php74.clone()).unwrap();
    let ext_for_php80 = get_package("ourcustom/ext-foobar", "1.0");
    fixtures.repo.add_package(ext_for_php80.clone()).unwrap();

    ext_for_php74
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "ourcustom/php".to_string(),
            link(
                "ourcustom/ext-foobar",
                "ourcustom/PHP",
                multi(vec![
                    get_version_constraint(">=", "7.4.0"),
                    get_version_constraint("<", "7.5.0"),
                ]),
                Link::TYPE_REQUIRE,
            ),
        )]));
    ext_for_php80
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "ourcustom/php".to_string(),
            link(
                "ourcustom/ext-foobar",
                "ourcustom/PHP",
                multi(vec![
                    get_version_constraint(">=", "8.0.0"),
                    get_version_constraint("<", "8.1.0"),
                ]),
                Link::TYPE_REQUIRE,
            ),
        )]));

    // reposComplete()
    let mut repo_set = fixtures.repo_set;
    repo_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();
    repo_set
        .add_repository(fixtures.repo_locked.clone().into())
        .unwrap();

    let mut request = fixtures.request;
    request.require_name("ourcustom/PHP", None).unwrap();
    request.require_name("ourcustom/ext-foobar", None).unwrap();

    check_solver_result_repo_set(
        &mut repo_set,
        request,
        vec![
            ExpectedJob::Single {
                job: "install",
                package: php80.clone(),
            },
            ExpectedJob::Single {
                job: "install",
                package: ext_for_php80,
            },
        ],
    );

    // now we flip the requirements around: we request "ext-foobar" before "php"
    let mut request = Request::new(Some(fixtures.repo_locked.clone()));
    request.require_name("ourcustom/ext-foobar", None).unwrap();
    request.require_name("ourcustom/PHP", None).unwrap();

    check_solver_result_repo_set(
        &mut repo_set,
        request,
        vec![
            ExpectedJob::Single {
                job: "install",
                package: php74,
            },
            ExpectedJob::Single {
                job: "install",
                package: ext_for_php74,
            },
        ],
    );
}

#[test]
fn test_solver_multi_package_name_version_resolution_is_independent_of_require_order_if_ordered_descending_by_requirement()
 {
    let fixtures = set_up();
    let php74 = get_package("ourcustom/PHP", "7.4");
    fixtures.repo.add_package(php74.clone()).unwrap();
    let php80 = get_package("ourcustom/PHP", "8.0");
    fixtures.repo.add_package(php80.clone()).unwrap();
    // note we are inserting this one into the repo first, unlike in the previous test
    let ext_for_php80 = get_package("ourcustom/ext-foobar", "1.0");
    fixtures.repo.add_package(ext_for_php80.clone()).unwrap();
    let ext_for_php74 = get_package("ourcustom/ext-foobar", "1.0");
    fixtures.repo.add_package(ext_for_php74.clone()).unwrap();

    ext_for_php80
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "ourcustom/php".to_string(),
            link(
                "ourcustom/ext-foobar",
                "ourcustom/PHP",
                multi(vec![
                    get_version_constraint(">=", "8.0.0"),
                    get_version_constraint("<", "8.1.0"),
                ]),
                Link::TYPE_REQUIRE,
            ),
        )]));
    ext_for_php74
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "ourcustom/php".to_string(),
            link(
                "ourcustom/ext-foobar",
                "ourcustom/PHP",
                multi(vec![
                    get_version_constraint(">=", "7.4.0"),
                    get_version_constraint("<", "7.5.0"),
                ]),
                Link::TYPE_REQUIRE,
            ),
        )]));

    // reposComplete()
    let mut repo_set = fixtures.repo_set;
    repo_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();
    repo_set
        .add_repository(fixtures.repo_locked.clone().into())
        .unwrap();

    let mut request = fixtures.request;
    request.require_name("ourcustom/PHP", None).unwrap();
    request.require_name("ourcustom/ext-foobar", None).unwrap();

    check_solver_result_repo_set(
        &mut repo_set,
        request,
        vec![
            ExpectedJob::Single {
                job: "install",
                package: php80.clone(),
            },
            ExpectedJob::Single {
                job: "install",
                package: ext_for_php80.clone(),
            },
        ],
    );

    // unlike in the previous test, the order of requirements no longer matters now
    let mut request = Request::new(Some(fixtures.repo_locked.clone()));
    request.require_name("ourcustom/ext-foobar", None).unwrap();
    request.require_name("ourcustom/PHP", None).unwrap();

    check_solver_result_repo_set(
        &mut repo_set,
        request,
        vec![
            ExpectedJob::Single {
                job: "install",
                package: php80,
            },
            ExpectedJob::Single {
                job: "install",
                package: ext_for_php80,
            },
        ],
    );
}

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

#[test]
fn test_solver_update_does_only_update() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo_locked.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo_locked.add_package(package_b.clone()).unwrap();
    let new_package_b = get_package("B", "1.1");
    fixtures.repo.add_package(new_package_b.clone()).unwrap();

    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                get_version_constraint(">=", "1.0.0.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    let mut request = fixtures.request;
    request.fix_package(package_a.clone());
    request
        .require_name("B", Some(get_version_constraint("=", "1.1.0.0")))
        .unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![ExpectedJob::Update {
            from: package_b,
            to: new_package_b,
        }],
    );
}

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

#[test]
fn test_solver_update_all() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo_locked.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo_locked.add_package(package_b.clone()).unwrap();
    let new_package_a = get_package("A", "1.1");
    fixtures.repo.add_package(new_package_a.clone()).unwrap();
    let new_package_b = get_package("B", "1.1");
    fixtures.repo.add_package(new_package_b.clone()).unwrap();

    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                MatchAllConstraint::new(None).into(),
                Link::TYPE_REQUIRE,
            ),
        )]));
    new_package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                MatchAllConstraint::new(None).into(),
                Link::TYPE_REQUIRE,
            ),
        )]));

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![
            ExpectedJob::Update {
                from: package_b,
                to: new_package_b,
            },
            ExpectedJob::Update {
                from: package_a,
                to: new_package_a,
            },
        ],
    );
}

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

#[test]
fn test_solver_all_jobs() {
    let fixtures = set_up();
    let package_d = get_package("D", "1.0");
    fixtures.repo_locked.add_package(package_d.clone()).unwrap();
    let old_package_c = get_package("C", "1.0");
    fixtures
        .repo_locked
        .add_package(old_package_c.clone())
        .unwrap();

    let package_a = get_package("A", "2.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();
    let new_package_b = get_package("B", "1.1");
    fixtures.repo.add_package(new_package_b.clone()).unwrap();
    let package_c = get_package("C", "1.1");
    fixtures.repo.add_package(package_c.clone()).unwrap();
    fixtures.repo.add_package(get_package("D", "1.0")).unwrap();
    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                get_version_constraint("<", "1.1"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();
    request.require_name("C", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![
            ExpectedJob::Single {
                job: "remove",
                package: package_d,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_b,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_a,
            },
            ExpectedJob::Update {
                from: old_package_c,
                to: package_c,
            },
        ],
    );
}

#[test]
fn test_solver_three_alternative_require_and_conflict() {
    let fixtures = set_up();
    let package_a = get_package("A", "2.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let middle_package_b = get_package("B", "1.0");
    fixtures.repo.add_package(middle_package_b.clone()).unwrap();
    let new_package_b = get_package("B", "1.1");
    fixtures.repo.add_package(new_package_b.clone()).unwrap();
    let old_package_b = get_package("B", "0.9");
    fixtures.repo.add_package(old_package_b.clone()).unwrap();
    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                get_version_constraint("<", "1.1"),
                Link::TYPE_REQUIRE,
            ),
        )]));
    package_a
        .as_complete_package()
        .unwrap()
        .__set_conflicts(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                get_version_constraint("<", "1.0"),
                Link::TYPE_CONFLICT,
            ),
        )]));

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![
            ExpectedJob::Single {
                job: "install",
                package: middle_package_b,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_a,
            },
        ],
    );
}

#[test]
fn test_solver_obsolete() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo_locked.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();
    package_b
        .as_complete_package()
        .unwrap()
        .__set_replaces(IndexMap::from([(
            "a".to_string(),
            link(
                "B",
                "A",
                MatchAllConstraint::new(None).into(),
                Link::TYPE_REPLACE,
            ),
        )]));

    let mut request = fixtures.request;
    request.require_name("B", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![
            ExpectedJob::Single {
                job: "remove",
                package: package_a,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_b,
            },
        ],
    );
}

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

#[test]
fn test_install_provider() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_q = get_package("Q", "1.0");
    fixtures.repo.add_package(package_q.clone()).unwrap();
    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));
    package_q
        .as_complete_package()
        .unwrap()
        .__set_provides(IndexMap::from([(
            "b".to_string(),
            link(
                "Q",
                "B",
                get_version_constraint("=", "1.0"),
                Link::TYPE_PROVIDE,
            ),
        )]));

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();

    // must explicitly pick the provider, so error in this case
    solve_expecting_error(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
    );
}

#[test]
fn test_skip_replacer_of_existing_package() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_q = get_package("Q", "1.0");
    fixtures.repo.add_package(package_q.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();
    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));
    package_q
        .as_complete_package()
        .unwrap()
        .__set_replaces(IndexMap::from([(
            "b".to_string(),
            link(
                "Q",
                "B",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_REPLACE,
            ),
        )]));

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![
            ExpectedJob::Single {
                job: "install",
                package: package_b,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_a,
            },
        ],
    );
}

#[test]
fn test_no_install_replacer_of_missing_package() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_q = get_package("Q", "1.0");
    fixtures.repo.add_package(package_q.clone()).unwrap();
    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));
    package_q
        .as_complete_package()
        .unwrap()
        .__set_replaces(IndexMap::from([(
            "b".to_string(),
            link(
                "Q",
                "B",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_REPLACE,
            ),
        )]));

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();

    solve_expecting_error(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
    );
}

#[test]
fn test_skip_replaced_package_if_replacer_is_selected() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_q = get_package("Q", "1.0");
    fixtures.repo.add_package(package_q.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();
    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));
    package_q
        .as_complete_package()
        .unwrap()
        .__set_replaces(IndexMap::from([(
            "b".to_string(),
            link(
                "Q",
                "B",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_REPLACE,
            ),
        )]));

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();
    request.require_name("Q", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![
            ExpectedJob::Single {
                job: "install",
                package: package_q,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_a,
            },
        ],
    );
}

#[test]
fn test_pick_older_if_newer_conflicts() {
    let fixtures = set_up();
    let package_x = get_package("X", "1.0");
    fixtures.repo.add_package(package_x.clone()).unwrap();
    package_x
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([
            (
                "a".to_string(),
                link(
                    "X",
                    "A",
                    get_version_constraint(">=", "2.0.0.0"),
                    Link::TYPE_REQUIRE,
                ),
            ),
            (
                "b".to_string(),
                link(
                    "X",
                    "B",
                    get_version_constraint(">=", "2.0.0.0"),
                    Link::TYPE_REQUIRE,
                ),
            ),
        ]));

    let package_a = get_package("A", "2.0.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let new_package_a = get_package("A", "2.1.0");
    fixtures.repo.add_package(new_package_a.clone()).unwrap();
    let new_package_b = get_package("B", "2.1.0");
    fixtures.repo.add_package(new_package_b.clone()).unwrap();

    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                get_version_constraint(">=", "2.0.0.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    // new package A depends on version of package B that does not exist
    // => new package A is not installable
    new_package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                get_version_constraint(">=", "2.2.0.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    // add a package S replacing both A and B, so that S and B or S and A cannot be simultaneously installed
    // but an alternative option for A and B both exists
    // this creates a more difficult so solve conflict
    let package_s = get_package("S", "2.0.0");
    fixtures.repo.add_package(package_s.clone()).unwrap();
    package_s
        .as_complete_package()
        .unwrap()
        .__set_replaces(IndexMap::from([
            (
                "a".to_string(),
                link(
                    "S",
                    "A",
                    get_version_constraint(">=", "2.0.0.0"),
                    Link::TYPE_REPLACE,
                ),
            ),
            (
                "b".to_string(),
                link(
                    "S",
                    "B",
                    get_version_constraint(">=", "2.0.0.0"),
                    Link::TYPE_REPLACE,
                ),
            ),
        ]));

    let mut request = fixtures.request;
    request.require_name("X", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![
            ExpectedJob::Single {
                job: "install",
                package: new_package_b,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_a,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_x,
            },
        ],
    );
}

#[test]
fn test_install_circular_require() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_b1 = get_package("B", "0.9");
    fixtures.repo.add_package(package_b1.clone()).unwrap();
    let package_b2 = get_package("B", "1.1");
    fixtures.repo.add_package(package_b2.clone()).unwrap();
    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));
    package_b2
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "a".to_string(),
            link(
                "B",
                "A",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![
            ExpectedJob::Single {
                job: "install",
                package: package_b2,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_a,
            },
        ],
    );
}

#[test]
fn test_install_alternative_with_circular_require() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();
    let package_c = get_package("C", "1.0");
    fixtures.repo.add_package(package_c.clone()).unwrap();
    let package_d = get_package("D", "1.0");
    fixtures.repo.add_package(package_d.clone()).unwrap();
    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));
    package_b
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "virtual".to_string(),
            link(
                "B",
                "Virtual",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));
    package_c
        .as_complete_package()
        .unwrap()
        .__set_provides(IndexMap::from([(
            "virtual".to_string(),
            link(
                "C",
                "Virtual",
                get_version_constraint("==", "1.0"),
                Link::TYPE_PROVIDE,
            ),
        )]));
    package_d
        .as_complete_package()
        .unwrap()
        .__set_provides(IndexMap::from([(
            "virtual".to_string(),
            link(
                "D",
                "Virtual",
                get_version_constraint("==", "1.0"),
                Link::TYPE_PROVIDE,
            ),
        )]));

    package_c
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "a".to_string(),
            link(
                "C",
                "A",
                get_version_constraint("==", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));
    package_d
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "a".to_string(),
            link(
                "D",
                "A",
                get_version_constraint("==", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();
    request.require_name("C", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![
            ExpectedJob::Single {
                job: "install",
                package: package_b,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_a,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_c,
            },
        ],
    );
}

#[test]
fn test_use_replacer_if_necessary() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();
    let package_d = get_package("D", "1.0");
    fixtures.repo.add_package(package_d.clone()).unwrap();
    let package_d2 = get_package("D", "1.1");
    fixtures.repo.add_package(package_d2.clone()).unwrap();

    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([
            (
                "b".to_string(),
                link(
                    "A",
                    "B",
                    get_version_constraint(">=", "1.0"),
                    Link::TYPE_REQUIRE,
                ),
            ),
            (
                "c".to_string(),
                link(
                    "A",
                    "C",
                    get_version_constraint(">=", "1.0"),
                    Link::TYPE_REQUIRE,
                ),
            ),
        ]));

    package_d
        .as_complete_package()
        .unwrap()
        .__set_replaces(IndexMap::from([
            (
                "b".to_string(),
                link(
                    "D",
                    "B",
                    get_version_constraint(">=", "1.0"),
                    Link::TYPE_REPLACE,
                ),
            ),
            (
                "c".to_string(),
                link(
                    "D",
                    "C",
                    get_version_constraint(">=", "1.0"),
                    Link::TYPE_REPLACE,
                ),
            ),
        ]));

    package_d2
        .as_complete_package()
        .unwrap()
        .__set_replaces(IndexMap::from([
            (
                "b".to_string(),
                link(
                    "D",
                    "B",
                    get_version_constraint(">=", "1.0"),
                    Link::TYPE_REPLACE,
                ),
            ),
            (
                "c".to_string(),
                link(
                    "D",
                    "C",
                    get_version_constraint(">=", "1.0"),
                    Link::TYPE_REPLACE,
                ),
            ),
        ]));

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();
    request.require_name("D", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![
            ExpectedJob::Single {
                job: "install",
                package: package_d2,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_a,
            },
        ],
    );
}

#[test]
fn test_issue265() {
    let fixtures = set_up();
    let package_a1 = get_package("A", "2.0.999999-dev");
    fixtures.repo.add_package(package_a1.clone()).unwrap();
    let package_a2 = get_package("A", "2.1-dev");
    fixtures.repo.add_package(package_a2.clone()).unwrap();
    let package_a3 = get_package("A", "2.2-dev");
    fixtures.repo.add_package(package_a3.clone()).unwrap();
    let package_b1 = get_package("B", "2.0.10");
    fixtures.repo.add_package(package_b1.clone()).unwrap();
    let package_b2 = get_package("B", "2.0.9");
    fixtures.repo.add_package(package_b2.clone()).unwrap();
    let package_c = get_package("C", "2.0-dev");
    fixtures.repo.add_package(package_c.clone()).unwrap();
    let package_d = get_package("D", "2.0.9");
    fixtures.repo.add_package(package_d.clone()).unwrap();

    package_c
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([
            (
                "a".to_string(),
                link(
                    "C",
                    "A",
                    get_version_constraint(">=", "2.0"),
                    Link::TYPE_REQUIRE,
                ),
            ),
            (
                "d".to_string(),
                link(
                    "C",
                    "D",
                    get_version_constraint(">=", "2.0"),
                    Link::TYPE_REQUIRE,
                ),
            ),
        ]));

    package_d
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([
            (
                "a".to_string(),
                link(
                    "D",
                    "A",
                    get_version_constraint(">=", "2.1"),
                    Link::TYPE_REQUIRE,
                ),
            ),
            (
                "b".to_string(),
                link(
                    "D",
                    "B",
                    get_version_constraint(">=", "2.0-dev"),
                    Link::TYPE_REQUIRE,
                ),
            ),
        ]));

    package_b1
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "a".to_string(),
            link(
                "B",
                "A",
                get_version_constraint("==", "2.1.0.0-dev"),
                Link::TYPE_REQUIRE,
            ),
        )]));
    package_b2
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "a".to_string(),
            link(
                "B",
                "A",
                get_version_constraint("==", "2.1.0.0-dev"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    package_b2
        .as_complete_package()
        .unwrap()
        .__set_replaces(IndexMap::from([(
            "d".to_string(),
            link(
                "B",
                "D",
                get_version_constraint("==", "2.0.9.0"),
                Link::TYPE_REPLACE,
            ),
        )]));

    let mut request = fixtures.request;
    request
        .require_name("C", Some(get_version_constraint("==", "2.0.0.0-dev")))
        .unwrap();

    solve_expecting_error(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
    );
}

#[test]
fn test_conflict_result_empty() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();
    package_a
        .as_complete_package()
        .unwrap()
        .__set_conflicts(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_CONFLICT,
            ),
        )]));

    let mut request = fixtures.request;
    request
        .require_name(
            "A",
            Some(MatchAllConstraint::new(Some("*".to_string())).into()),
        )
        .unwrap();
    request
        .require_name(
            "B",
            Some(MatchAllConstraint::new(Some("*".to_string())).into()),
        )
        .unwrap();

    let SolveError {
        exception,
        repo_set,
        request,
        pool,
    } = solve_expecting_problems(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
    );

    assert_eq!(exception.get_problems().len(), 1);

    let msg = "\n  Problem 1\n    - Root composer.json requires a * -> satisfiable by A[1.0].\n    - Root composer.json requires b * -> satisfiable by B[1.0].\n    - A 1.0 conflicts with B 1.0.\n";
    assert_eq!(
        exception
            .get_pretty_string(&repo_set, &request, &mut pool.borrow_mut(), false, false)
            .unwrap(),
        msg
    );
}

#[test]
fn test_unsatisfiable_requires() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();

    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                get_version_constraint(">=", "2.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();

    let SolveError {
        exception,
        repo_set,
        request,
        pool,
    } = solve_expecting_problems(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
    );

    assert_eq!(exception.get_problems().len(), 1);

    let msg = "\n  Problem 1\n    - Root composer.json requires a * -> satisfiable by A[1.0].\n    - A 1.0 requires b >= 2.0 -> found B[1.0] but it does not match the constraint.\n";
    assert_eq!(
        exception
            .get_pretty_string(&repo_set, &request, &mut pool.borrow_mut(), false, false)
            .unwrap(),
        msg
    );
}

#[test]
fn test_require_mismatch_exception() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();
    fixtures.repo.add_package(get_package("B", "0.9")).unwrap();
    let package_c = get_package("C", "1.0");
    fixtures.repo.add_package(package_c.clone()).unwrap();
    let package_d = get_package("D", "1.0");
    fixtures.repo.add_package(package_d.clone()).unwrap();

    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "A",
                "B",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));
    package_b
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "c".to_string(),
            link(
                "B",
                "C",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));
    package_c
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "d".to_string(),
            link(
                "C",
                "D",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));
    package_d
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link(
                "D",
                "B",
                get_version_constraint("<", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    let mut request = fixtures.request;
    request
        .require_name(
            "A",
            Some(MatchAllConstraint::new(Some("*".to_string())).into()),
        )
        .unwrap();

    let SolveError {
        exception,
        repo_set,
        request,
        pool,
    } = solve_expecting_problems(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
    );

    assert_eq!(exception.get_problems().len(), 1);

    let msg = "\n  Problem 1\n    - Root composer.json requires a * -> satisfiable by A[1.0].\n    - A 1.0 requires b >= 1.0 -> satisfiable by B[1.0].\n    - B 1.0 requires c >= 1.0 -> satisfiable by C[1.0].\n    - C 1.0 requires d >= 1.0 -> satisfiable by D[1.0].\n    - D 1.0 requires b < 1.0 -> satisfiable by B[0.9].\n    - You can only install one version of a package, so only one of these can be installed: B[0.9, 1.0].\n";
    assert_eq!(
        exception
            .get_pretty_string(&repo_set, &request, &mut pool.borrow_mut(), false, false)
            .unwrap(),
        msg
    );
}

#[test]
fn test_learn_literals_with_sorted_rule_literals() {
    let fixtures = set_up();
    let package_twig2 = get_package("twig/twig", "2.0");
    fixtures.repo.add_package(package_twig2.clone()).unwrap();
    let package_twig16 = get_package("twig/twig", "1.6");
    fixtures.repo.add_package(package_twig16.clone()).unwrap();
    let package_twig15 = get_package("twig/twig", "1.5");
    fixtures.repo.add_package(package_twig15.clone()).unwrap();
    let package_symfony = get_package("symfony/symfony", "2.0");
    fixtures.repo.add_package(package_symfony.clone()).unwrap();
    let package_twig_bridge = get_package("symfony/twig-bridge", "2.0");
    fixtures
        .repo
        .add_package(package_twig_bridge.clone())
        .unwrap();

    package_twig_bridge
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "twig/twig".to_string(),
            link(
                "symfony/twig-bridge",
                "twig/twig",
                get_version_constraint("<", "2.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    package_symfony
        .as_complete_package()
        .unwrap()
        .__set_replaces(IndexMap::from([(
            "symfony/twig-bridge".to_string(),
            link(
                "symfony/symfony",
                "symfony/twig-bridge",
                get_version_constraint("==", "2.0"),
                Link::TYPE_REPLACE,
            ),
        )]));

    let mut request = fixtures.request;
    request.require_name("symfony/twig-bridge", None).unwrap();
    request.require_name("twig/twig", None).unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![
            ExpectedJob::Single {
                job: "install",
                package: package_twig16,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_twig_bridge,
            },
        ],
    );
}

#[test]
fn test_install_recursive_alias_dependencies() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "2.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();
    let package_a2 = get_package("A", "2.0");
    fixtures.repo.add_package(package_a2.clone()).unwrap();

    package_a2
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "b".to_string(),
            link_pretty(
                "A",
                "B",
                get_version_constraint("==", "2.0"),
                Link::TYPE_REQUIRE,
                "== 2.0",
            ),
        )]));
    package_b
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "a".to_string(),
            link(
                "B",
                "A",
                get_version_constraint(">=", "2.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    let package_a2_alias = get_alias_package(&package_a2, "1.1");
    fixtures.repo.add_package(package_a2_alias.clone()).unwrap();

    let mut request = fixtures.request;
    request
        .require_name("A", Some(get_version_constraint("==", "1.1.0.0")))
        .unwrap();

    check_solver_result(
        fixtures.repo_set,
        fixtures.repo,
        fixtures.repo_locked,
        request,
        vec![
            ExpectedJob::Single {
                job: "install",
                package: package_b,
            },
            ExpectedJob::Single {
                job: "install",
                package: package_a2,
            },
            ExpectedJob::Single {
                job: "markAliasInstalled",
                package: package_a2_alias,
            },
        ],
    );
}

#[test]
fn test_install_dev_alias() {
    let fixtures = set_up();
    let package_a = get_package("A", "2.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();

    package_b
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "a".to_string(),
            link(
                "B",
                "A",
                get_version_constraint("<", "2.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    let package_a_alias = get_alias_package(&package_a, "1.1");
    fixtures.repo.add_package(package_a_alias.clone()).unwrap();

    let mut request = fixtures.request;
    request
        .require_name("A", Some(get_version_constraint("==", "2.0")))
        .unwrap();
    request.require_name("B", None).unwrap();

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
        ],
    );
}

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

#[test]
fn test_learn_positive_literal() {
    let fixtures = set_up();
    let package_a = get_package("A", "1.0");
    fixtures.repo.add_package(package_a.clone()).unwrap();
    let package_b = get_package("B", "1.0");
    fixtures.repo.add_package(package_b.clone()).unwrap();
    let package_c1 = get_package("C", "1.0");
    fixtures.repo.add_package(package_c1.clone()).unwrap();
    let package_c2 = get_package("C", "2.0");
    fixtures.repo.add_package(package_c2.clone()).unwrap();
    let package_d = get_package("D", "1.0");
    fixtures.repo.add_package(package_d.clone()).unwrap();
    let package_e = get_package("E", "1.0");
    fixtures.repo.add_package(package_e.clone()).unwrap();
    let package_f1 = get_package("F", "1.0");
    fixtures.repo.add_package(package_f1.clone()).unwrap();
    let package_f2 = get_package("F", "2.0");
    fixtures.repo.add_package(package_f2.clone()).unwrap();
    let package_g1 = get_package("G", "1.0");
    fixtures.repo.add_package(package_g1.clone()).unwrap();
    let package_g2 = get_package("G", "2.0");
    fixtures.repo.add_package(package_g2.clone()).unwrap();
    let package_g3 = get_package("G", "3.0");
    fixtures.repo.add_package(package_g3.clone()).unwrap();

    package_a
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([
            (
                "b".to_string(),
                link(
                    "A",
                    "B",
                    get_version_constraint("==", "1.0"),
                    Link::TYPE_REQUIRE,
                ),
            ),
            (
                "c".to_string(),
                link(
                    "A",
                    "C",
                    get_version_constraint(">=", "1.0"),
                    Link::TYPE_REQUIRE,
                ),
            ),
            (
                "d".to_string(),
                link(
                    "A",
                    "D",
                    get_version_constraint("==", "1.0"),
                    Link::TYPE_REQUIRE,
                ),
            ),
        ]));

    package_b
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "e".to_string(),
            link(
                "B",
                "E",
                get_version_constraint("==", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    package_c1
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "f".to_string(),
            link(
                "C",
                "F",
                get_version_constraint("==", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));
    package_c2
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([
            (
                "f".to_string(),
                link(
                    "C",
                    "F",
                    get_version_constraint("==", "1.0"),
                    Link::TYPE_REQUIRE,
                ),
            ),
            (
                "g".to_string(),
                link(
                    "C",
                    "G",
                    get_version_constraint(">=", "1.0"),
                    Link::TYPE_REQUIRE,
                ),
            ),
        ]));

    package_d
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "f".to_string(),
            link(
                "D",
                "F",
                get_version_constraint(">=", "1.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    package_e
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "g".to_string(),
            link(
                "E",
                "G",
                get_version_constraint("<=", "2.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    // reposComplete() + createSolver() inline so the testFlagLearnedPositiveLiteral flag can be
    // asserted on the same solver instance used to solve.
    let mut repo_set = fixtures.repo_set;
    repo_set
        .add_repository(RepositoryInterfaceHandle::new(fixtures.repo))
        .unwrap();
    repo_set
        .add_repository(fixtures.repo_locked.into())
        .unwrap();

    let mut request = fixtures.request;
    request.require_name("A", None).unwrap();

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let pool = repo_set
        .create_pool(&mut request, io.clone(), None, None, vec![], None, None)
        .unwrap();
    let policy: std::rc::Rc<dyn PolicyInterface> =
        std::rc::Rc::new(DefaultPolicy::new(false, false, None));
    let mut solver = shirabe::dependency_resolver::solver::Solver::new(
        policy,
        std::rc::Rc::new(std::cell::RefCell::new(pool)),
        io,
    );

    // check correct setup for assertion later
    assert!(!solver.test_flag_learned_positive_literal);

    let transaction = solver.solve(&request, None).unwrap().unwrap();

    let expected = vec![
        ("install".to_string(), package_f1.get_unique_name()),
        ("install".to_string(), package_d.get_unique_name()),
        ("install".to_string(), package_g2.get_unique_name()),
        ("install".to_string(), package_c2.get_unique_name()),
        ("install".to_string(), package_e.get_unique_name()),
        ("install".to_string(), package_b.get_unique_name()),
        ("install".to_string(), package_a.get_unique_name()),
    ];
    let mut result: Vec<(String, String)> = Vec::new();
    for operation in transaction.get_operations() {
        if let Some(update) = operation.as_update_operation() {
            result.push((
                "update".to_string(),
                format!(
                    "{} => {}",
                    update.get_initial_package().get_unique_name(),
                    update.get_target_package().get_unique_name()
                ),
            ));
        } else {
            let op_type = operation.get_operation_type();
            let job = if op_type == "uninstall" {
                "remove".to_string()
            } else {
                op_type
            };
            result.push((job, operation.get_package().get_unique_name()));
        }
    }
    assert_eq!(expected, result);

    // verify that the code path leading to a negative literal resulting in a positive learned
    // literal is actually executed
    assert!(solver.test_flag_learned_positive_literal);
}
