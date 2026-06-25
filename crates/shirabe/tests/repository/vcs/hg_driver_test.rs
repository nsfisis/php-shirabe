//! ref: composer/tests/Composer/Test/Repository/Vcs/HgDriverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::HgDriver;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::http_downloader::HttpDownloaderMockHandler;
use shirabe::util::process_executor::MockHandler;
use shirabe_php_shim::{PhpMixed, RuntimeException};
use tempfile::TempDir;

use crate::http_downloader_mock::{HttpDownloaderMockGuard, get_http_downloader_mock};
use crate::io_stub::IOStub;
use crate::process_executor_mock::{ProcessExecutorMockGuard, cmd_full, get_process_executor_mock};

struct SetUp {
    home: TempDir,
    config: Config,
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

    SetUp { home, config }
}

fn tear_down(home: &std::path::Path) {
    let mut fs = Filesystem::new(None);
    fs.remove_directory(home).unwrap();
}

struct TearDown {
    home: std::path::PathBuf,
}

impl TearDown {
    fn new(home: std::path::PathBuf) -> Self {
        TearDown { home }
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.home);
    }
}

fn supports_data_provider() -> Vec<&'static str> {
    vec![
        "ssh://bitbucket.org/user/repo",
        "ssh://hg@bitbucket.org/user/repo",
        "ssh://user@bitbucket.org/user/repo",
        "https://bitbucket.org/user/repo",
        "https://user@bitbucket.org/user/repo",
    ]
}

#[test]
fn test_supports() {
    for repository_url in supports_data_provider() {
        let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
        let config = Rc::new(RefCell::new(Config::new(true, None)));

        assert!(HgDriver::supports(io, config, repository_url, false).unwrap());
    }
}

#[test]
fn test_get_branches_filter_invalid_branch_names() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let config = Rc::new(RefCell::new(config));
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(IOStub::new()));

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) =
        get_http_downloader_mock(vec![], false, HttpDownloaderMockHandler::default());

    let stdout = "default 1:dbf6c8acb640\n--help  1:dbf6c8acb640";
    let stdout1 = "help    1:dbf6c8acb641\n--help  1:dbf6c8acb641\n";

    let (process, _process_guard): (_, ProcessExecutorMockGuard) = get_process_executor_mock(
        vec![
            cmd_full(["hg", "branches"], 0, stdout, ""),
            cmd_full(["hg", "bookmarks"], 0, stdout1, ""),
        ],
        false,
        MockHandler::default(),
    );

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert(
        "url".to_string(),
        PhpMixed::String("https://example.org/acme.git".to_string()),
    );

    let mut driver = HgDriver::new(repo_config, io, config, http_downloader, process);

    let branches = driver.get_branches().unwrap();
    let mut expected: IndexMap<String, String> = IndexMap::new();
    expected.insert("help".to_string(), "dbf6c8acb641".to_string());
    expected.insert("default".to_string(), "dbf6c8acb640".to_string());
    assert_eq!(expected, branches);
}

#[test]
fn test_file_get_content_invalid_identifier() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

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

    let driver = HgDriver::new(repo_config, io, config, http_downloader, process);

    assert_eq!(None, driver.get_file_content("file.txt", "h").unwrap());

    let err = driver.get_file_content("file.txt", "-h").unwrap_err();
    assert!(err.downcast_ref::<RuntimeException>().is_some());
}

#[test]
fn test_get_change_date_invalid_identifier() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

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

    let driver = HgDriver::new(repo_config, io, config, http_downloader, process);

    let err = driver.get_change_date("-r foo").unwrap_err();
    assert!(err.downcast_ref::<RuntimeException>().is_some());
}
