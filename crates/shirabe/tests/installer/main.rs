#[path = "../common/async_runtime.rs"]
mod async_runtime;
#[path = "../common/bootstrap.rs"]
mod bootstrap;
#[path = "../common/io_mock.rs"]
mod io_mock;
#[path = "../common/test_case.rs"]
mod test_case;

mod binary_installer_test;
mod installation_manager_test;
mod installer_event_test;
mod library_installer_test;
mod metapackage_installer_test;
mod suggested_packages_reporter_test;
