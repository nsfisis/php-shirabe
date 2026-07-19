#[path = "../common/bootstrap.rs"]
mod bootstrap;
#[path = "../common/config_stub.rs"]
mod config_stub;
#[path = "../common/io_stub.rs"]
mod io_stub;
#[path = "../common/process_executor_mock.rs"]
mod process_executor_mock;
#[path = "../common/test_case.rs"]
mod test_case;

mod archiver;
mod base_package_test;
mod complete_package_test;
mod dumper;
mod loader;
mod locker_test;
mod root_alias_package_test;
mod version;
