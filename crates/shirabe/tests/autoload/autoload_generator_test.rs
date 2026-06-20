//! ref: composer/tests/Composer/Test/Autoload/AutoloadGeneratorTest.php

// These exercise AutoloadGenerator end-to-end: they build packages, write fixture files to
// a temp dir, run dump() through a mocked InstalledRepository/EventDispatcher and compare
// the generated autoload files. The integration setup (fixtures, mocks, filesystem) is not
// ported yet.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_root_package_autoloading);
stub!(test_root_package_dev_autoloading);
stub!(test_root_package_dev_autoloading_disabled_by_default);
stub!(test_vendor_dir_same_as_working_dir);
stub!(test_root_package_autoloading_alternative_vendor_dir);
stub!(test_root_package_autoloading_with_target_dir);
stub!(test_duplicate_files_warning);
stub!(test_vendors_autoloading);
stub!(test_vendors_autoloading_with_metapackages);
stub!(test_non_dev_autoload_exclusion_with_recursion);
stub!(test_non_dev_autoload_should_include_replaced_packages);
stub!(test_non_dev_autoload_exclusion_with_recursion_replace);
stub!(test_non_dev_autoload_replaces_nested_requirements);
stub!(test_phar_autoload);
stub!(test_psr_to_class_map_ignores_non_existing_dir);
stub!(test_psr_to_class_map_ignores_non_psr_classes);
stub!(test_vendors_class_map_autoloading);
stub!(test_vendors_class_map_autoloading_with_target_dir);
stub!(test_class_map_autoloading_empty_dir_and_exact_file);
stub!(test_class_map_autoloading_authoritative_and_apcu);
stub!(test_class_map_autoloading_authoritative_and_apcu_prefix);
stub!(test_files_autoload_generation);
stub!(test_files_autoload_generation_remove_extra_entities_from_autoload_files);
stub!(test_files_autoload_order_by_dependencies);
stub!(test_override_vendors_autoloading);
stub!(test_include_path_file_generation);
stub!(test_include_paths_are_prepended_in_autoload_file);
stub!(test_include_paths_in_root_package);
stub!(test_include_path_file_without_paths_is_skipped);
stub!(test_pre_and_post_events_are_dispatched_during_autoload_dump);
stub!(test_use_global_include_path);
stub!(test_vendor_dir_excluded_from_working_dir);
stub!(test_up_level_relative_paths);
stub!(test_autoload_rules_in_package_that_does_not_exist_on_disk);
stub!(test_empty_paths);
stub!(test_vendor_substring_path);
stub!(test_exclude_from_classmap);
stub!(test_generates_platform_check);
stub!(test_absolute_symlink_with_psr4_does_not_generate_warnings);
stub!(test_absolute_symlink_with_classmap_exclude_from_classmap);
