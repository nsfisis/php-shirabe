//! ref: composer/tests/Composer/Test/Repository/Vcs/GitDriverTest.php

use crate::http_downloader_mock::{HttpDownloaderMockGuard, get_http_downloader_mock};
use crate::io_stub::IOStub;
use crate::process_executor_mock::{ProcessExecutorMockGuard, cmd_full, get_process_executor_mock};
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::repository::vcs::GitDriver;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::http_downloader::HttpDownloaderMockHandler;
use shirabe::util::platform::Platform;
use shirabe::util::process_executor::MockHandler;
use shirabe_php_shim::{PhpMixed, RuntimeException};
use std::cell::RefCell;
use std::rc::Rc;
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
#[serial]
fn test_get_root_identifier_from_remote_local_repository() {
    let SetUp {
        home,
        config,
        network_env,
    } = set_up();
    let home_path = home.path().to_string_lossy().into_owned();
    let _tear_down = TearDown::new(home.path().to_path_buf(), network_env);

    let config = Rc::new(RefCell::new(config));
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(IOStub::new()));

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) =
        get_http_downloader_mock(vec![], false, HttpDownloaderMockHandler::default());

    let stdout = "* main\n  2.2\n  1.10";

    let (process, _process_guard): (_, ProcessExecutorMockGuard) = get_process_executor_mock(
        vec![cmd_full(["git", "branch", "--no-color"], 0, stdout, "")],
        true,
        MockHandler::default(),
    );

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert("url".to_string(), PhpMixed::String(home_path.clone()));

    let mut driver = GitDriver::new(repo_config, io, config, http_downloader, process);
    driver.__set_repo_dir(home_path);

    assert_eq!("main", driver.get_root_identifier().unwrap());
}

#[test]
#[serial]
fn test_get_root_identifier_from_remote() {
    let SetUp {
        home,
        config,
        network_env,
    } = set_up();
    let home_path = home.path().to_string_lossy().into_owned();
    let _tear_down = TearDown::new(home.path().to_path_buf(), network_env);

    let config = Rc::new(RefCell::new(config));
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(IOStub::new()));

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) =
        get_http_downloader_mock(vec![], false, HttpDownloaderMockHandler::default());

    let stdout = "* remote origin\n  Fetch URL: https://example.org/acme.git\n  Push  URL: https://example.org/acme.git\n  HEAD branch: main\n  Remote branches:\n    1.10                       tracked\n    2.2                        tracked\n    main                       tracked";

    let (process, _process_guard): (_, ProcessExecutorMockGuard) = get_process_executor_mock(
        vec![
            cmd_full(["git", "remote", "-v"], 0, "", ""),
            cmd_full(
                [
                    "git",
                    "remote",
                    "set-url",
                    "origin",
                    "--",
                    "https://example.org/acme.git",
                ],
                0,
                "",
                "",
            ),
            cmd_full(["git", "remote", "show", "origin"], 0, stdout, ""),
            cmd_full(
                [
                    "git",
                    "remote",
                    "set-url",
                    "origin",
                    "--",
                    "https://example.org/acme.git",
                ],
                0,
                "",
                "",
            ),
        ],
        false,
        MockHandler::default(),
    );

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert(
        "url".to_string(),
        PhpMixed::String("https://example.org/acme.git".to_string()),
    );

    let mut driver = GitDriver::new(repo_config, io, config, http_downloader, process);
    driver.__set_repo_dir(home_path);

    assert_eq!("main", driver.get_root_identifier().unwrap());
}

#[test]
#[serial]
fn test_get_root_identifier_from_local_with_network_disabled() {
    let SetUp {
        home,
        config,
        network_env,
    } = set_up();
    let home_path = home.path().to_string_lossy().into_owned();
    let _tear_down = TearDown::new(home.path().to_path_buf(), network_env);

    Platform::put_env("COMPOSER_DISABLE_NETWORK", "1");

    let config = Rc::new(RefCell::new(config));
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(IOStub::new()));

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) =
        get_http_downloader_mock(vec![], false, HttpDownloaderMockHandler::default());

    let stdout = "* main\n  2.2\n  1.10";

    let (process, _process_guard): (_, ProcessExecutorMockGuard) = get_process_executor_mock(
        vec![cmd_full(["git", "branch", "--no-color"], 0, stdout, "")],
        false,
        MockHandler::default(),
    );

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert(
        "url".to_string(),
        PhpMixed::String("https://example.org/acme.git".to_string()),
    );

    let mut driver = GitDriver::new(repo_config, io, config, http_downloader, process);
    driver.__set_repo_dir(home_path);

    assert_eq!("main", driver.get_root_identifier().unwrap());
}

#[test]
fn test_get_branches_filter_invalid_branch_names() {
    let SetUp {
        home,
        config,
        network_env,
    } = set_up();
    let home_path = home.path().to_string_lossy().into_owned();
    let _tear_down = TearDown::new(home.path().to_path_buf(), network_env);

    let config = Rc::new(RefCell::new(config));
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(IOStub::new()));

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) =
        get_http_downloader_mock(vec![], false, HttpDownloaderMockHandler::default());

    // Branches starting with a - character are not valid git branch names.
    // Still assert that they get filtered to prevent issues later on.
    let stdout = "* main 089681446ba44d6d9004350192486f2ceb4eaa06 commit\n  2.2  12681446ba44d6d9004350192486f2ceb4eaa06 commit\n  -h   089681446ba44d6d9004350192486f2ceb4eaa06 commit";

    let (process, _process_guard): (_, ProcessExecutorMockGuard) = get_process_executor_mock(
        vec![cmd_full(
            ["git", "branch", "--no-color", "--no-abbrev", "-v"],
            0,
            stdout,
            "",
        )],
        false,
        MockHandler::default(),
    );

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert(
        "url".to_string(),
        PhpMixed::String("https://example.org/acme.git".to_string()),
    );

    let mut driver = GitDriver::new(repo_config, io, config, http_downloader, process);
    driver.__set_repo_dir(home_path);

    let branches = driver.get_branches().unwrap();
    let mut expected: IndexMap<String, String> = IndexMap::new();
    expected.insert(
        "main".to_string(),
        "089681446ba44d6d9004350192486f2ceb4eaa06".to_string(),
    );
    expected.insert(
        "2.2".to_string(),
        "12681446ba44d6d9004350192486f2ceb4eaa06".to_string(),
    );
    assert_eq!(expected, branches);
}

#[test]
fn test_file_get_content_invalid_identifier() {
    let SetUp {
        home,
        config,
        network_env,
    } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf(), network_env);

    let config = Rc::new(RefCell::new(config));
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(IOStub::new()));

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) =
        get_http_downloader_mock(vec![], false, HttpDownloaderMockHandler::default());

    let (process, _process_guard): (_, ProcessExecutorMockGuard) =
        get_process_executor_mock(vec![], false, MockHandler::default());

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert(
        "url".to_string(),
        PhpMixed::String("https://example.org/acme.git".to_string()),
    );

    let mut driver = GitDriver::new(repo_config, io, config, http_downloader, process);

    assert_eq!(None, driver.get_file_content("file.txt", "h").unwrap());

    let err = driver.get_file_content("file.txt", "-h").unwrap_err();
    assert!(err.downcast_ref::<RuntimeException>().is_some());
}

#[test]
fn test_get_change_date_invalid_identifier() {
    let SetUp {
        home,
        config,
        network_env,
    } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf(), network_env);

    let config = Rc::new(RefCell::new(config));
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(IOStub::new()));

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) =
        get_http_downloader_mock(vec![], false, HttpDownloaderMockHandler::default());

    let (process, _process_guard): (_, ProcessExecutorMockGuard) =
        get_process_executor_mock(vec![], false, MockHandler::default());

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert(
        "url".to_string(),
        PhpMixed::String("https://example.org/acme.git".to_string()),
    );

    let mut driver = GitDriver::new(repo_config, io, config, http_downloader, process);

    let err = driver.get_change_date("-n1 --format=%at HEAD").unwrap_err();
    assert!(err.downcast_ref::<RuntimeException>().is_some());
}
