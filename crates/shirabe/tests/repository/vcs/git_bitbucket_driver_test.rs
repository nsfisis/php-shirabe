//! ref: composer/tests/Composer/Test/Repository/Vcs/GitBitbucketDriverTest.php

use crate::http_downloader_mock::{HttpDownloaderMockGuard, expect_full, get_http_downloader_mock};
use crate::io_stub::IOStub;
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::GitBitbucketDriver;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::http_downloader::{HttpDownloader, HttpDownloaderMockHandler};
use shirabe::util::process_executor::ProcessExecutor;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed, RuntimeException};
use tempfile::TempDir;

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

// Mirrors PHP's $httpDownloader->expects([...], true): an `['url' => .., 'body' => ..]`
// entry (status defaults to 200).
fn http_body(
    url: &str,
    body: impl Into<String>,
) -> shirabe::util::http_downloader::HttpDownloaderMockExpectation {
    expect_full(url, None, 200, body, vec![String::new()])
}

// Mirrors PHP's getDriver: constructs the driver from the given repo url with a mocked
// IOInterface, the test config, the mocked HttpDownloader, and a real ProcessExecutor,
// then calls initialize().
fn get_driver(
    url: &str,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
) -> anyhow::Result<GitBitbucketDriver> {
    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert("url".to_string(), PhpMixed::String(url.to_string()));

    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
        io.clone(),
    ))));

    let mut driver = GitBitbucketDriver::new(repo_config, io, config, http_downloader, process);
    driver.initialize()?;
    Ok(driver)
}

#[test]
fn test_get_root_identifier_wrong_scm_type() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let config = std::rc::Rc::new(std::cell::RefCell::new(config));
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()));

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) = get_http_downloader_mock(
        vec![http_body(
            "https://api.bitbucket.org/2.0/repositories/user/repo?fields=-project%2C-owner",
            r#"{"scm":"hg","website":"","has_wiki":false,"name":"repo","links":{"branches":{"href":"https:\/\/api.bitbucket.org\/2.0\/repositories\/user\/repo\/refs\/branches"},"tags":{"href":"https:\/\/api.bitbucket.org\/2.0\/repositories\/user\/repo\/refs\/tags"},"clone":[{"href":"https:\/\/user@bitbucket.org\/user\/repo","name":"https"},{"href":"ssh:\/\/hg@bitbucket.org\/user\/repo","name":"ssh"}],"html":{"href":"https:\/\/bitbucket.org\/user\/repo"}},"language":"php","created_on":"2015-02-18T16:22:24.688+00:00","updated_on":"2016-05-17T13:20:21.993+00:00","is_private":true,"has_issues":false}"#,
        )],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let mut driver = get_driver(
        "https://bitbucket.org/user/repo.git",
        io,
        config,
        http_downloader,
    )
    .unwrap();

    let err = driver.get_root_identifier().unwrap_err();
    let runtime = err
        .downcast_ref::<RuntimeException>()
        .expect("expected RuntimeException");
    assert_eq!(
        "https://bitbucket.org/user/repo.git does not appear to be a git repository, use https://bitbucket.org/user/repo but remember that Bitbucket no longer supports the mercurial repositories. https://bitbucket.org/blog/sunsetting-mercurial-support-in-bitbucket",
        runtime.message
    );
}

#[test]
fn test_driver() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let config = std::rc::Rc::new(std::cell::RefCell::new(config));
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()));

    let urls = [
        "https://api.bitbucket.org/2.0/repositories/user/repo?fields=-project%2C-owner",
        "https://api.bitbucket.org/2.0/repositories/user/repo/refs/tags?pagelen=100&fields=values.name%2Cvalues.target.hash%2Cnext&sort=-target.date",
        "https://api.bitbucket.org/2.0/repositories/user/repo/refs/branches?pagelen=100&fields=values.name%2Cvalues.target.hash%2Cvalues.heads%2Cnext&sort=-target.date",
        "https://api.bitbucket.org/2.0/repositories/user/repo/src/main/composer.json",
        "https://api.bitbucket.org/2.0/repositories/user/repo/commit/main?fields=date",
    ];
    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![
            http_body(
                urls[0],
                r#"{"mainbranch": {"name": "main"}, "scm":"git","website":"","has_wiki":false,"name":"repo","links":{"branches":{"href":"https:\/\/api.bitbucket.org\/2.0\/repositories\/user\/repo\/refs\/branches"},"tags":{"href":"https:\/\/api.bitbucket.org\/2.0\/repositories\/user\/repo\/refs\/tags"},"clone":[{"href":"https:\/\/user@bitbucket.org\/user\/repo.git","name":"https"},{"href":"ssh:\/\/git@bitbucket.org\/user\/repo.git","name":"ssh"}],"html":{"href":"https:\/\/bitbucket.org\/user\/repo"}},"language":"php","created_on":"2015-02-18T16:22:24.688+00:00","updated_on":"2016-05-17T13:20:21.993+00:00","is_private":true,"has_issues":false}"#,
            ),
            http_body(
                urls[1],
                r#"{"values":[{"name":"1.0.1","target":{"hash":"9b78a3932143497c519e49b8241083838c8ff8a1"}},{"name":"1.0.0","target":{"hash":"d3393d514318a9267d2f8ebbf463a9aaa389f8eb"}}]}"#,
            ),
            http_body(
                urls[2],
                r#"{"values":[{"name":"main","target":{"hash":"937992d19d72b5116c3e8c4a04f960e5fa270b22"}}]}"#,
            ),
            http_body(
                urls[3],
                r#"{"name": "user/repo","description": "test repo","license": "GPL","authors": [{"name": "Name","email": "local@domain.tld"}],"require": {"creator/package": "^1.0"},"require-dev": {"phpunit/phpunit": "~4.8"}}"#,
            ),
            http_body(urls[4], r#"{"date": "2016-05-17T13:19:52+00:00"}"#),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let mut driver = get_driver(
        "https://bitbucket.org/user/repo.git",
        io,
        config,
        http_downloader,
    )
    .unwrap();

    assert_eq!("main", driver.get_root_identifier().unwrap());

    let mut expected_tags: IndexMap<String, String> = IndexMap::new();
    expected_tags.insert(
        "1.0.1".to_string(),
        "9b78a3932143497c519e49b8241083838c8ff8a1".to_string(),
    );
    expected_tags.insert(
        "1.0.0".to_string(),
        "d3393d514318a9267d2f8ebbf463a9aaa389f8eb".to_string(),
    );
    assert_eq!(expected_tags, driver.get_tags().unwrap());

    let mut expected_branches: IndexMap<String, String> = IndexMap::new();
    expected_branches.insert(
        "main".to_string(),
        "937992d19d72b5116c3e8c4a04f960e5fa270b22".to_string(),
    );
    assert_eq!(expected_branches, driver.get_branches().unwrap());

    let data = driver.get_composer_information("main").unwrap().unwrap();
    assert_eq!(
        Some("user/repo"),
        data.get("name").and_then(|v| v.as_string())
    );
    assert_eq!(
        Some("test repo"),
        data.get("description").and_then(|v| v.as_string())
    );
    assert_eq!(
        Some("2016-05-17T13:19:52+00:00"),
        data.get("time").and_then(|v| v.as_string())
    );
    let support = data.get("support").and_then(|v| v.as_array()).unwrap();
    assert_eq!(
        Some(
            "https://bitbucket.org/user/repo/src/937992d19d72b5116c3e8c4a04f960e5fa270b22/?at=main"
        ),
        support.get("source").and_then(|v| v.as_string())
    );
    assert_eq!(
        Some("https://bitbucket.org/user/repo"),
        data.get("homepage").and_then(|v| v.as_string())
    );

    // testGetParams (PHP @depends testDriver): assertions over the same driver.
    let url = "https://bitbucket.org/user/repo.git";
    assert_eq!(url, driver.get_url());

    let dist = driver.get_dist("reference").unwrap();
    assert_eq!(Some("zip"), dist.get("type").map(String::as_str));
    assert_eq!(
        Some("https://bitbucket.org/user/repo/get/reference.zip"),
        dist.get("url").map(String::as_str)
    );
    assert_eq!(Some("reference"), dist.get("reference").map(String::as_str));
    assert_eq!(Some(""), dist.get("shasum").map(String::as_str));

    let source = driver.get_source("reference");
    assert_eq!(Some("git"), source.get("type").map(String::as_str));
    assert_eq!(Some(url), source.get("url").map(String::as_str));
    assert_eq!(
        Some("reference"),
        source.get("reference").map(String::as_str)
    );
}

#[test]
fn test_initialize_invalid_repository_url() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let config = std::rc::Rc::new(std::cell::RefCell::new(config));
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()));

    let (http_downloader, _http_guard) =
        get_http_downloader_mock(vec![], true, HttpDownloaderMockHandler::default());

    let result = get_driver("https://bitbucket.org/acme", io, config, http_downloader);
    let err = result.unwrap_err();
    assert!(
        err.downcast_ref::<InvalidArgumentException>().is_some(),
        "expected InvalidArgumentException, got: {err:?}"
    );
}

#[test]
fn test_invalid_support_data() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let config = std::rc::Rc::new(std::cell::RefCell::new(config));
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()));

    let repo_url = "https://bitbucket.org/user/repo.git";

    let urls = [
        "https://api.bitbucket.org/2.0/repositories/user/repo?fields=-project%2C-owner",
        "https://api.bitbucket.org/2.0/repositories/user/repo/src/main/composer.json",
        "https://api.bitbucket.org/2.0/repositories/user/repo/commit/main?fields=date",
        "https://api.bitbucket.org/2.0/repositories/user/repo/refs/tags?pagelen=100&fields=values.name%2Cvalues.target.hash%2Cnext&sort=-target.date",
        "https://api.bitbucket.org/2.0/repositories/user/repo/refs/branches?pagelen=100&fields=values.name%2Cvalues.target.hash%2Cvalues.heads%2Cnext&sort=-target.date",
    ];
    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![
            http_body(
                urls[0],
                r#"{"mainbranch": {"name": "main"}, "scm":"git","website":"","has_wiki":false,"name":"repo","links":{"branches":{"href":"https:\/\/api.bitbucket.org\/2.0\/repositories\/user\/repo\/refs\/branches"},"tags":{"href":"https:\/\/api.bitbucket.org\/2.0\/repositories\/user\/repo\/refs\/tags"},"clone":[{"href":"https:\/\/user@bitbucket.org\/user\/repo.git","name":"https"},{"href":"ssh:\/\/git@bitbucket.org\/user\/repo.git","name":"ssh"}],"html":{"href":"https:\/\/bitbucket.org\/user\/repo"}},"language":"php","created_on":"2015-02-18T16:22:24.688+00:00","updated_on":"2016-05-17T13:20:21.993+00:00","is_private":true,"has_issues":false}"#,
            ),
            http_body(urls[1], format!(r#"{{"support": "{repo_url}"}}"#)),
            http_body(urls[2], r#"{"date": "2016-05-17T13:19:52+00:00"}"#),
            http_body(
                urls[3],
                r#"{"values":[{"name":"1.0.1","target":{"hash":"9b78a3932143497c519e49b8241083838c8ff8a1"}},{"name":"1.0.0","target":{"hash":"d3393d514318a9267d2f8ebbf463a9aaa389f8eb"}}]}"#,
            ),
            http_body(
                urls[4],
                r#"{"values":[{"name":"main","target":{"hash":"937992d19d72b5116c3e8c4a04f960e5fa270b22"}}]}"#,
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let mut driver = get_driver(repo_url, io, config, http_downloader).unwrap();

    driver.get_root_identifier().unwrap();
    let data = driver.get_composer_information("main").unwrap().unwrap();

    let support = data.get("support").and_then(|v| v.as_array()).unwrap();
    assert_eq!(
        Some(
            "https://bitbucket.org/user/repo/src/937992d19d72b5116c3e8c4a04f960e5fa270b22/?at=main"
        ),
        support.get("source").and_then(|v| v.as_string())
    );
}

#[test]
fn test_supports() {
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(true, None)));

    assert!(
        GitBitbucketDriver::supports(
            io.clone(),
            config.clone(),
            "https://bitbucket.org/user/repo.git",
            false
        )
        .unwrap()
    );

    // should not be changed, see https://github.com/composer/composer/issues/9400
    assert!(
        !GitBitbucketDriver::supports(
            io.clone(),
            config.clone(),
            "git@bitbucket.org:user/repo.git",
            false
        )
        .unwrap()
    );

    assert!(
        !GitBitbucketDriver::supports(io, config, "https://github.com/user/repo.git", false)
            .unwrap()
    );
}
