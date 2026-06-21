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
#[ignore = "PluginInstaller and runtime loading of fixture plugin PHP classes (plugin-v1) are not implemented (TODO(plugin))"]
#[test]
fn test_install_new_plugin() {
    todo!()
}

#[ignore = "PluginInstaller and runtime loading of fixture plugin PHP classes are not implemented (TODO(plugin))"]
#[test]
fn test_install_plugin_with_root_package_having_files_autoload() {
    todo!()
}

#[ignore = "PluginInstaller and runtime loading of fixture plugin PHP classes (plugin-v4) are not implemented (TODO(plugin))"]
#[test]
fn test_install_multiple_plugins() {
    todo!()
}

#[ignore = "PluginInstaller.update and runtime plugin class loading/deactivation are not implemented (TODO(plugin))"]
#[test]
fn test_upgrade_with_new_class_name() {
    todo!()
}

#[ignore = "PluginInstaller.uninstall and runtime plugin class loading/uninstall hook are not implemented (TODO(plugin))"]
#[test]
fn test_uninstall() {
    todo!()
}

#[ignore = "PluginInstaller.update and runtime plugin class loading/deactivation are not implemented (TODO(plugin))"]
#[test]
fn test_upgrade_with_same_class_name() {
    todo!()
}

#[ignore = "PluginInstaller and runtime loading of fixture plugin PHP classes are not implemented (TODO(plugin))"]
#[test]
fn test_register_plugin_only_one_time() {
    todo!()
}

#[ignore = "Requires mocking getPluginApiVersion and runtime loading of fixture plugin PHP classes; not implemented (TODO(plugin))"]
#[test]
fn test_star_plugin_version_works_with_any_api_version() {
    todo!()
}

#[ignore = "Requires mocking getPluginApiVersion and runtime loading of fixture plugin PHP classes; not implemented (TODO(plugin))"]
#[test]
fn test_plugin_constraint_works_only_with_certain_api_version() {
    todo!()
}

#[ignore = "Requires mocking getPluginApiVersion and runtime loading of fixture plugin PHP classes; not implemented (TODO(plugin))"]
#[test]
fn test_plugin_range_constraints_work_only_with_certain_api_version() {
    todo!()
}

#[ignore = "get_plugin_capabilities and runtime loading of fixture plugin/capability PHP classes (plugin-v8) are not implemented (TODO(plugin))"]
#[test]
fn test_command_provider_capability() {
    todo!()
}

#[ignore = "Requires a PHP-runtime mock of PluginInterface and get_plugin_capability; not implemented (TODO(plugin))"]
#[test]
fn test_incapable_plugin_is_correctly_detected() {
    todo!()
}

#[ignore = "Requires runtime instantiation of Mock\\Capability via get_plugin_capability; not implemented (TODO(plugin))"]
#[test]
fn test_capability_implements_composer_plugin_api_class_and_is_constructed_with_args() {
    todo!()
}

#[ignore = "Requires runtime get_plugin_capability with a mocked Capable plugin and PHP-class-name capability lookup; not implemented (TODO(plugin))"]
#[test]
fn test_querying_with_invalid_capability_class_name_throws() {
    todo!()
}

#[ignore = "Requires runtime get_plugin_capability with a mocked Capable plugin; not implemented (TODO(plugin))"]
#[test]
fn test_querying_non_provided_capability_returns_null_safely() {
    todo!()
}

#[ignore = "Requires runtime get_plugin_capability with PHP-class-name capability lookup; not implemented (TODO(plugin))"]
#[test]
fn test_querying_with_non_existing_or_wrong_capability_class_types_throws() {
    todo!()
}
