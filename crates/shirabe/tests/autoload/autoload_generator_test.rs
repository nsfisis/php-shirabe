//! ref: composer/tests/Composer/Test/Autoload/AutoloadGeneratorTest.php

/// Creates the working/vendor temp directories, switches into the working dir, and
/// builds the AutoloadGenerator from a mocked Config/InstallationManager/
/// InstalledRepository/EventDispatcher and a BufferIO. The mocks and temp-dir
/// helpers are not available here, so this remains a stub.
fn set_up() {
    todo!()
}

/// Restores the original working directory and removes the working/vendor
/// directories created by `set_up`, which is itself a stub.
fn tear_down() {
    todo!()
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

// These exercise AutoloadGenerator end-to-end: they build packages, write fixture files to
// a temp dir, run dump() through a mocked InstalledRepository/EventDispatcher and compare
// the generated autoload files. The integration setup (fixtures, mocks, filesystem) is not
// ported yet.
#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_root_package_autoloading() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_root_package_dev_autoloading() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_root_package_dev_autoloading_disabled_by_default() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_vendor_dir_same_as_working_dir() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_root_package_autoloading_alternative_vendor_dir() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_root_package_autoloading_with_target_dir() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_duplicate_files_warning() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_vendors_autoloading() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_vendors_autoloading_with_metapackages() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_non_dev_autoload_exclusion_with_recursion() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_non_dev_autoload_should_include_replaced_packages() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_non_dev_autoload_exclusion_with_recursion_replace() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_non_dev_autoload_replaces_nested_requirements() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_phar_autoload() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_psr_to_class_map_ignores_non_existing_dir() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_psr_to_class_map_ignores_non_psr_classes() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_vendors_class_map_autoloading() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_vendors_class_map_autoloading_with_target_dir() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_class_map_autoloading_empty_dir_and_exact_file() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_class_map_autoloading_authoritative_and_apcu() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_class_map_autoloading_authoritative_and_apcu_prefix() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_files_autoload_generation() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_files_autoload_generation_remove_extra_entities_from_autoload_files() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_files_autoload_order_by_dependencies() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_override_vendors_autoloading() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_include_path_file_generation() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_include_paths_are_prepended_in_autoload_file() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_include_paths_in_root_package() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_include_path_file_without_paths_is_skipped() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_pre_and_post_events_are_dispatched_during_autoload_dump() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_use_global_include_path() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_vendor_dir_excluded_from_working_dir() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_up_level_relative_paths() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_autoload_rules_in_package_that_does_not_exist_on_disk() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_empty_paths() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_vendor_substring_path() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_exclude_from_classmap() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_generates_platform_check() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_absolute_symlink_with_psr4_does_not_generate_warnings() {
    todo!()
}

#[test]
#[ignore = "not yet ported (AutoloadGenerator integration: fixtures, mocked installers and generated-file comparison)"]
fn test_absolute_symlink_with_classmap_exclude_from_classmap() {
    todo!()
}
