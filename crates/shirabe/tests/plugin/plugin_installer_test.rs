//! ref: composer/tests/Composer/Test/Plugin/PluginInstallerTest.php

// The plugin system requires the PHP runtime to load and instantiate plugin classes; the
// PluginManager/PluginInstaller is intentionally not implemented yet (TODO(plugin)).
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_install_new_plugin);
stub!(test_install_plugin_with_root_package_having_files_autoload);
stub!(test_install_multiple_plugins);
stub!(test_upgrade_with_new_class_name);
stub!(test_uninstall);
stub!(test_upgrade_with_same_class_name);
stub!(test_register_plugin_only_one_time);
stub!(test_star_plugin_version_works_with_any_api_version);
stub!(test_plugin_constraint_works_only_with_certain_api_version);
stub!(test_plugin_range_constraints_work_only_with_certain_api_version);
stub!(test_command_provider_capability);
stub!(test_incapable_plugin_is_correctly_detected);
stub!(test_capability_implements_composer_plugin_api_class_and_is_constructed_with_args);
stub!(test_querying_with_invalid_capability_class_name_throws);
stub!(test_querying_non_provided_capability_returns_null_safely);
stub!(test_querying_with_non_existing_or_wrong_capability_class_types_throws);
