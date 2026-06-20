//! ref: composer/tests/Composer/Test/DependencyResolver/SolverTest.php

// These run the dependency Solver over packages/requests built from version constraints,
// whose parsing goes through a look-around regex the regex crate cannot compile; the setup
// also mirrors the larger solver fixtures.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "not yet ported (runs the Solver; constraint parsing uses a look-around regex)"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_solver_install_single);
stub!(test_solver_remove_if_not_requested);
stub!(test_install_non_existing_package_fails);
stub!(test_solver_install_same_package_from_different_repositories);
stub!(test_solver_install_with_deps);
stub!(test_solver_install_honours_not_equal_operator);
stub!(test_solver_install_with_deps_in_order);
stub!(test_solver_multi_package_name_version_resolution_depends_on_require_order);
stub!(test_solver_multi_package_name_version_resolution_is_independent_of_require_order_if_ordered_descending_by_requirement);
stub!(test_solver_fix_locked);
stub!(test_solver_fix_locked_with_alternative);
stub!(test_solver_update_does_only_update);
stub!(test_solver_update_single);
stub!(test_solver_update_all);
stub!(test_solver_update_current);
stub!(test_solver_update_only_updates_selected_package);
stub!(test_solver_update_constrained);
stub!(test_solver_update_fully_constrained);
stub!(test_solver_update_fully_constrained_prunes_installed_packages);
stub!(test_solver_all_jobs);
stub!(test_solver_three_alternative_require_and_conflict);
stub!(test_solver_obsolete);
stub!(test_install_one_of_two_alternatives);
stub!(test_install_provider);
stub!(test_skip_replacer_of_existing_package);
stub!(test_no_install_replacer_of_missing_package);
stub!(test_skip_replaced_package_if_replacer_is_selected);
stub!(test_pick_older_if_newer_conflicts);
stub!(test_install_circular_require);
stub!(test_install_alternative_with_circular_require);
stub!(test_use_replacer_if_necessary);
stub!(test_issue265);
stub!(test_conflict_result_empty);
stub!(test_unsatisfiable_requires);
stub!(test_require_mismatch_exception);
stub!(test_learn_literals_with_sorted_rule_literals);
stub!(test_install_recursive_alias_dependencies);
stub!(test_install_dev_alias);
stub!(test_install_root_aliases_if_alias_of_is_installed);
stub!(test_learn_positive_literal);
