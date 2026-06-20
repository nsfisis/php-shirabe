//! ref: composer/tests/Composer/Test/Command/RemoveCommandTest.php

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_exception_running_with_no_remove_packages);
stub!(test_exception_when_running_unused_without_lock_file);
stub!(test_warning_when_removing_non_existent_package);
stub!(test_warning_when_removing_package_from_wrong_type);
stub!(test_warning_when_removing_package_with_deprecated_dependencies_flag);
stub!(test_message_output_when_no_unused_packages_to_remove);
stub!(test_remove_unused_package);
stub!(test_remove_package_by_name);
stub!(test_remove_package_by_name_with_dry_run);
stub!(test_remove_allowed_plugin_package_with_no_other_allowed_plugins);
stub!(test_remove_allowed_plugin_package_with_other_allowed_plugins);
stub!(test_remove_packages_by_vendor);
stub!(test_remove_packages_by_vendor_with_dry_run);
stub!(test_warning_when_removing_packages_by_vendor_from_wrong_type);
stub!(test_package_still_present_error_when_no_install_flag_used);
stub!(test_update_inherited_dependencies_flag_is_passed_to_post_remove_installer);
