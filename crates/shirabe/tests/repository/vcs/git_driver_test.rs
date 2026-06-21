//! ref: composer/tests/Composer/Test/Repository/Vcs/GitDriverTest.php

// Every case constructs a GitDriver with a mocked ProcessExecutor (and an HttpDownloader
// that reaches curl_multi_init, todo!()) to feed git command output; mocking is not
// available here.

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::platform::Platform;
use shirabe_php_shim::PhpMixed;
use tempfile::TempDir;

struct SetUp {
    home: TempDir,
    config: Config,
    network_env: Option<String>,
}

fn set_up() -> SetUp {
    let home = TempDir::new().unwrap();
    let mut config = Config::new(true, None);
    let mut top: IndexMap<String, PhpMixed> = IndexMap::new();
    let mut config_section: IndexMap<String, PhpMixed> = IndexMap::new();
    config_section.insert(
        "home".to_string(),
        PhpMixed::String(home.path().to_string_lossy().into_owned()),
    );
    top.insert("config".to_string(), PhpMixed::Array(config_section));
    config.merge(&top, Config::SOURCE_UNKNOWN);
    let network_env = Platform::get_env("COMPOSER_DISABLE_NETWORK");

    SetUp {
        home,
        config,
        network_env,
    }
}

fn tear_down(home: &std::path::Path, network_env: &Option<String>) {
    let mut fs = Filesystem::new(None);
    fs.remove_directory(home).unwrap();
    match network_env {
        None => Platform::clear_env("COMPOSER_DISABLE_NETWORK"),
        Some(network_env) => Platform::put_env("COMPOSER_DISABLE_NETWORK", network_env),
    }
}

struct TearDown {
    home: std::path::PathBuf,
    network_env: Option<String>,
}

impl TearDown {
    fn new(home: std::path::PathBuf, network_env: Option<String>) -> Self {
        TearDown { home, network_env }
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.home, &self.network_env);
    }
}

#[test]
#[ignore = "requires ProcessExecutor mock (getProcessExecutorMock/expects) and Reflection setRepoDir, neither available"]
fn test_get_root_identifier_from_remote_local_repository() {
    let SetUp {
        home,
        config: _,
        network_env,
    } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf(), network_env);
    todo!()
}

#[test]
#[ignore = "requires ProcessExecutor mock (getProcessExecutorMock/expects), IO mock and Reflection setRepoDir, none available"]
fn test_get_root_identifier_from_remote() {
    let SetUp {
        home,
        config: _,
        network_env,
    } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf(), network_env);
    todo!()
}

#[test]
#[ignore = "requires ProcessExecutor mock (getProcessExecutorMock/expects) and Reflection setRepoDir, neither available"]
fn test_get_root_identifier_from_local_with_network_disabled() {
    let SetUp {
        home,
        config: _,
        network_env,
    } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf(), network_env);
    todo!()
}

#[test]
#[ignore = "requires ProcessExecutor mock (getProcessExecutorMock/expects), IOInterface mock and Reflection setRepoDir, none available"]
fn test_get_branches_filter_invalid_branch_names() {
    let SetUp {
        home,
        config: _,
        network_env,
    } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf(), network_env);
    todo!()
}

#[test]
#[ignore = "requires ProcessExecutor mock (getProcessExecutorMock) and IOInterface/HttpDownloader mocks, none available"]
fn test_file_get_content_invalid_identifier() {
    let SetUp {
        home,
        config: _,
        network_env,
    } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf(), network_env);
    todo!()
}

#[test]
#[ignore = "requires ProcessExecutor mock (getProcessExecutorMock) and IOInterface/HttpDownloader mocks, none available"]
fn test_get_change_date_invalid_identifier() {
    let SetUp {
        home,
        config: _,
        network_env,
    } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf(), network_env);
    todo!()
}
