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

mod auth_helper_test;
mod bitbucket_test;
mod config_validator_test;
mod error_handler_test;
mod filesystem_test;
mod forgejo_test;
mod forgejo_url_test;
mod git_test;
mod github_test;
mod gitlab_test;
mod http;
mod http_downloader_test;
mod ini_helper_test;
mod metadata_minifier_test;
mod no_proxy_pattern_test;
mod package_sorter_test;
mod perforce_test;
mod platform_test;
mod process_executor_test;
mod remote_filesystem_test;
mod silencer_test;
mod stream_context_factory_test;
mod svn_test;
mod tar_test;
mod tls_helper_test;
mod url_test;
mod zip_test;
