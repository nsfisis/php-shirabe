#[path = "../common/async_runtime.rs"]
mod async_runtime;
#[path = "../common/config_stub.rs"]
mod config_stub;
#[path = "../common/http_downloader_mock.rs"]
mod http_downloader_mock;
#[path = "../common/io_mock.rs"]
mod io_mock;
#[path = "../common/io_stub.rs"]
mod io_stub;
#[path = "../common/process_executor_mock.rs"]
mod process_executor_mock;

mod archive_downloader_test;
mod download_manager_test;
mod file_downloader_test;
mod fossil_downloader_test;
mod git_downloader_test;
mod hg_downloader_test;
mod perforce_downloader_test;
mod xz_downloader_test;
mod zip_downloader_test;
