//! ref: composer/tests/Composer/Test/Command/ShowCommandTest.php

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_show);
stub!(test_outdated_filters_according_to_platform_reqs_and_warns);
stub!(test_outdated_filters_according_to_platform_reqs_without_warning_for_higher_versions);
stub!(test_show_direct_with_name_does_not_show_transient_dependencies);
stub!(test_show_direct_with_name_only_shows_direct_dependents);
stub!(test_show_platform_only_shows_platform_packages);
stub!(test_show_platform_works_without_composer_json);
stub!(test_outdated_with_zero_major);
stub!(test_show_all_shows_all_sections);
stub!(test_locked_requires_valid_lock_file);
stub!(test_locked_shows_all_locked);
stub!(test_invalid_option_combinations);
stub!(test_ignored_option_combinations);
stub!(test_self_and_name_only);
stub!(test_self_and_package_combination);
stub!(test_self);
stub!(test_not_installed_error);
stub!(test_no_dev_option);
stub!(test_package_filter);
stub!(test_not_existing_package);
stub!(test_not_existing_package_with_working_dir);
stub!(test_specific_package_and_tree);
stub!(test_name_only_prints_no_trailing_whitespace);
