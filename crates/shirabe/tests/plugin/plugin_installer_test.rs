//! ref: composer/tests/Composer/Test/Plugin/PluginInstallerTest.php

/// Builds the Composer instance from mocked DownloadManager/RepositoryManager/
/// InstallationManager/EventDispatcher and an InstalledRepository mock, plus the
/// plugin fixtures temp directory. The mocks and the plugin machinery are not
/// available here, so this remains a stub.
fn set_up() {
    todo!()
}

/// Removes the fixtures directory created by `set_up`, which is itself a stub.
fn tear_down() {
    todo!()
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

// The plugin system requires the PHP runtime to load and instantiate plugin classes; the
// PluginManager/PluginInstaller is intentionally not implemented yet (TODO(plugin)).
#[test]
#[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
fn test_install_new_plugin() {
    todo!()
}

#[test]
#[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
fn test_install_plugin_with_root_package_having_files_autoload() {
    todo!()
}

#[test]
#[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
fn test_install_multiple_plugins() {
    todo!()
}

#[test]
#[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
fn test_upgrade_with_new_class_name() {
    todo!()
}

#[test]
#[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
fn test_uninstall() {
    todo!()
}

#[test]
#[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
fn test_upgrade_with_same_class_name() {
    todo!()
}

#[test]
#[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
fn test_register_plugin_only_one_time() {
    todo!()
}

#[test]
#[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
fn test_star_plugin_version_works_with_any_api_version() {
    todo!()
}

#[test]
#[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
fn test_plugin_constraint_works_only_with_certain_api_version() {
    todo!()
}

#[test]
#[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
fn test_plugin_range_constraints_work_only_with_certain_api_version() {
    todo!()
}

#[test]
#[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
fn test_command_provider_capability() {
    todo!()
}

#[test]
#[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
fn test_incapable_plugin_is_correctly_detected() {
    todo!()
}

#[test]
#[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
fn test_capability_implements_composer_plugin_api_class_and_is_constructed_with_args() {
    todo!()
}

#[test]
#[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
fn test_querying_with_invalid_capability_class_name_throws() {
    todo!()
}

#[test]
#[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
fn test_querying_non_provided_capability_returns_null_safely() {
    todo!()
}

#[test]
#[ignore = "the plugin API (PluginManager/PluginInstaller loading PHP plugin classes) is not implemented yet"]
fn test_querying_with_non_existing_or_wrong_capability_class_types_throws() {
    todo!()
}
