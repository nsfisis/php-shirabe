//! ref: composer/tests/Composer/Test/Repository/Vcs/ForgejoDriverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::ForgejoDriver;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::http_downloader::{HttpDownloader, HttpDownloaderMockHandler};
use shirabe::util::process_executor::{MockHandler, ProcessExecutor};
use shirabe_php_shim::PhpMixed;
use tempfile::TempDir;

use crate::http_downloader_mock::{HttpDownloaderMockGuard, expect_full, get_http_downloader_mock};
use crate::process_executor_mock::{ProcessExecutorMockGuard, get_process_executor_mock};

struct SetUp {
    home: TempDir,
    config: Rc<RefCell<Config>>,
    io: Rc<RefCell<dyn IOInterface>>,
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
    config_section.insert(
        "forgejo-domains".to_string(),
        PhpMixed::List(vec![PhpMixed::String("codeberg.org".to_string())]),
    );
    top.insert("config".to_string(), PhpMixed::Array(config_section));
    config.merge(&top, Config::SOURCE_UNKNOWN);

    // PHP mocks IOInterface with isInteractive() => true; on the passing (200) paths
    // exercised here the interactivity flag is never consulted, so a bare NullIO matches.
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));

    SetUp {
        home,
        config: Rc::new(RefCell::new(config)),
        io,
    }
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

// ref: ForgejoDriverTest::initializeDriver
fn initialize_driver(
    set_up: &SetUp,
    repo_url: &str,
    http_downloader: Rc<RefCell<HttpDownloader>>,
) -> (ForgejoDriver, ProcessExecutorMockGuard) {
    let (process, process_guard) = get_process_executor_mock(vec![], false, MockHandler::default());

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert("url".to_string(), PhpMixed::String(repo_url.to_string()));
    let mut driver = ForgejoDriver::new(
        repo_config,
        set_up.io.clone(),
        set_up.config.clone(),
        http_downloader,
        process,
    );
    driver.initialize().unwrap();

    (driver, process_guard)
}

fn http_mock(
    expectations: Vec<shirabe::util::http_downloader::HttpDownloaderMockExpectation>,
) -> (Rc<RefCell<HttpDownloader>>, HttpDownloaderMockGuard) {
    get_http_downloader_mock(expectations, true, HttpDownloaderMockHandler::default())
}

fn repo_body() -> String {
    shirabe::json::JsonFile::encode(&PhpMixed::Array({
        let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
        m.insert(
            "default_branch".to_string(),
            PhpMixed::String("main".to_string()),
        );
        m.insert("has_issues".to_string(), PhpMixed::Bool(true));
        m.insert("archived".to_string(), PhpMixed::Bool(false));
        m.insert("private".to_string(), PhpMixed::Bool(false));
        m.insert(
            "html_url".to_string(),
            PhpMixed::String("https://codeberg.org/acme/repo".to_string()),
        );
        m.insert(
            "ssh_url".to_string(),
            PhpMixed::String("git@codeberg.org:acme/repo.git".to_string()),
        );
        m.insert(
            "clone_url".to_string(),
            PhpMixed::String("https://codeberg.org/acme/repo.git".to_string()),
        );
        m
    }))
}

#[test]
fn test_public_repository() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.home.path().to_path_buf());

    let (http_downloader, _http_guard) = http_mock(vec![expect_full(
        "https://codeberg.org/api/v1/repos/acme/repo",
        None,
        200,
        repo_body(),
        vec![String::new()],
    )]);

    let (mut driver, _process_guard) = initialize_driver(
        &set_up,
        "https://codeberg.org/acme/repo.git",
        http_downloader,
    );
    assert_eq!("main", driver.get_root_identifier().unwrap());

    let sha = "SOMESHA";
    let dist = driver.get_dist(sha).unwrap();
    assert_eq!("zip", dist.get("type").unwrap());
    assert_eq!(
        "https://codeberg.org/api/v1/repos/acme/repo/archive/SOMESHA.zip",
        dist.get("url").unwrap()
    );
    assert_eq!(sha, dist.get("reference").unwrap());

    let source = driver.get_source(sha);
    assert_eq!("git", source.get("type").unwrap());
    assert_eq!(
        "https://codeberg.org/acme/repo.git",
        source.get("url").unwrap()
    );
    assert_eq!(sha, source.get("reference").unwrap());
}

#[test]
fn test_get_branches() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.home.path().to_path_buf());

    let branches_body = shirabe::json::JsonFile::encode(&PhpMixed::List(vec![PhpMixed::Array({
        let mut commit: IndexMap<String, PhpMixed> = IndexMap::new();
        commit.insert("id".to_string(), PhpMixed::String("SOMESHA".to_string()));
        let mut entry: IndexMap<String, PhpMixed> = IndexMap::new();
        entry.insert("name".to_string(), PhpMixed::String("main".to_string()));
        entry.insert("commit".to_string(), PhpMixed::Array(commit));
        entry
    })]));

    let (http_downloader, _http_guard) = http_mock(vec![
        expect_full(
            "https://codeberg.org/api/v1/repos/acme/repo",
            None,
            200,
            repo_body(),
            vec![String::new()],
        ),
        expect_full(
            "https://codeberg.org/api/v1/repos/acme/repo/branches?per_page=100",
            None,
            200,
            branches_body,
            vec![String::new()],
        ),
    ]);

    let (mut driver, _process_guard) = initialize_driver(
        &set_up,
        "https://codeberg.org/acme/repo.git",
        http_downloader,
    );

    let mut expected: IndexMap<String, String> = IndexMap::new();
    expected.insert("main".to_string(), "SOMESHA".to_string());
    assert_eq!(expected, driver.get_branches().unwrap());
}

#[test]
fn test_get_tags() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.home.path().to_path_buf());

    let tags_body = shirabe::json::JsonFile::encode(&PhpMixed::List(vec![PhpMixed::Array({
        let mut commit: IndexMap<String, PhpMixed> = IndexMap::new();
        commit.insert("sha".to_string(), PhpMixed::String("SOMESHA".to_string()));
        let mut entry: IndexMap<String, PhpMixed> = IndexMap::new();
        entry.insert("name".to_string(), PhpMixed::String("1.0".to_string()));
        entry.insert("commit".to_string(), PhpMixed::Array(commit));
        entry
    })]));

    let (http_downloader, _http_guard) = http_mock(vec![
        expect_full(
            "https://codeberg.org/api/v1/repos/acme/repo",
            None,
            200,
            repo_body(),
            vec![String::new()],
        ),
        expect_full(
            "https://codeberg.org/api/v1/repos/acme/repo/tags?per_page=100",
            None,
            200,
            tags_body,
            vec![String::new()],
        ),
    ]);

    let (mut driver, _process_guard) = initialize_driver(
        &set_up,
        "https://codeberg.org/acme/repo.git",
        http_downloader,
    );

    let mut expected: IndexMap<String, String> = IndexMap::new();
    expected.insert("1.0".to_string(), "SOMESHA".to_string());
    assert_eq!(expected, driver.get_tags().unwrap());
}

#[test]
fn test_get_empty_file_content() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.home.path().to_path_buf());

    let (http_downloader, _http_guard) = http_mock(vec![
        expect_full(
            "https://codeberg.org/api/v1/repos/acme/repo",
            None,
            200,
            repo_body(),
            vec![String::new()],
        ),
        expect_full(
            "https://codeberg.org/api/v1/repos/acme/repo/contents/composer.json?ref=main",
            None,
            200,
            "{\"encoding\":\"base64\",\"content\":\"\"}",
            vec![String::new()],
        ),
    ]);

    let (mut driver, _process_guard) = initialize_driver(
        &set_up,
        "https://codeberg.org/acme/repo.git",
        http_downloader,
    );

    assert_eq!(
        Some(String::new()),
        driver.get_file_content("composer.json", "main").unwrap()
    );
}

fn supports_provider() -> Vec<(bool, &'static str)> {
    vec![
        (false, "https://example.org/acme/repo"),
        (true, "https://codeberg.org/acme/repository"),
    ]
}

#[test]
fn test_supports() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.home.path().to_path_buf());

    for (expected, repo_url) in supports_provider() {
        assert_eq!(
            expected,
            ForgejoDriver::supports(set_up.io.clone(), set_up.config.clone(), repo_url, false)
                .unwrap()
        );
    }
}
