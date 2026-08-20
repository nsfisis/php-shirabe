#[path = "../common/async_runtime.rs"]
mod async_runtime;
#[path = "../common/config_stub.rs"]
mod config_stub;
#[path = "../common/php_worker.rs"]
mod php_worker;

mod alias_package_test;
mod e2e_command_provider_test;
mod e2e_extension_installer_test;
mod e2e_installer_test;
mod e2e_installers_test;
mod e2e_normalize_test;
mod e2e_package_event_test;
mod e2e_script_command_test;
mod e2e_script_event_test;
mod plugin_installer_test;
mod subscriber_test;
mod value_round_trip_test;
