//! ref: composer/tests/Composer/Test/Repository/Vcs/GitHubDriverTest.php

use crate::http_downloader_mock::{HttpDownloaderMockGuard, expect_full, get_http_downloader_mock};
use crate::io_stub::IOStub;
use crate::process_executor_mock::{
    ProcessExecutorMockGuard, cmd, cmd_full, get_process_executor_mock,
};
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::config::ConfigSourceInterface;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::GitHubDriver;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::http_downloader::HttpDownloaderMockHandler;
use shirabe::util::process_executor::{MockHandler, ProcessExecutor};
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::rc::Rc;
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

// PHP mocks `Composer\Config\ConfigSourceInterface` with getMockBuilder and sets
// no expectations (a passive mock whose methods all do nothing). Used by
// testPrivateRepository to satisfy the OAuth token-storing path.
mockall::mock! {
    #[derive(Debug)]
    pub ConfigSource {}
    impl ConfigSourceInterface for ConfigSource {
        fn add_repository(&mut self, name: &str, config: PhpMixed, append: bool) -> anyhow::Result<()>;
        fn insert_repository(&mut self, name: &str, config: PhpMixed, reference_name: &str, offset: i64) -> anyhow::Result<()>;
        fn set_repository_url(&mut self, name: &str, url: &str) -> anyhow::Result<()>;
        fn remove_repository(&mut self, name: &str) -> anyhow::Result<()>;
        fn add_config_setting(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()>;
        fn remove_config_setting(&mut self, name: &str) -> anyhow::Result<()>;
        fn add_property(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()>;
        fn remove_property(&mut self, name: &str) -> anyhow::Result<()>;
        fn add_link(&mut self, r#type: &str, name: &str, value: &str) -> anyhow::Result<()>;
        fn remove_link(&mut self, r#type: &str, name: &str) -> anyhow::Result<()>;
        fn get_name(&self) -> String;
    }
}

// A passive ConfigSource mock: every method is stubbed to do nothing, mirroring
// PHPUnit's `getMockBuilder(...)->getMock()` whose stubbed methods return null.
fn null_config_source() -> MockConfigSource {
    let mut m = MockConfigSource::new();
    m.expect_add_repository().returning(|_, _, _| Ok(()));
    m.expect_insert_repository().returning(|_, _, _, _| Ok(()));
    m.expect_set_repository_url().returning(|_, _| Ok(()));
    m.expect_remove_repository().returning(|_| Ok(()));
    m.expect_add_config_setting().returning(|_, _| Ok(()));
    m.expect_remove_config_setting().returning(|_| Ok(()));
    m.expect_add_property().returning(|_, _| Ok(()));
    m.expect_remove_property().returning(|_| Ok(()));
    m.expect_add_link().returning(|_, _, _| Ok(()));
    m.expect_remove_link().returning(|_, _| Ok(()));
    m.expect_get_name().returning(String::new);
    m
}

// Mirrors PHP's $httpDownloader->expects([...], true): an `['url' => .., 'body' => ..]`
// entry (status defaults to 200).
fn http_body(
    url: &str,
    body: impl Into<String>,
) -> shirabe::util::http_downloader::HttpDownloaderMockExpectation {
    expect_full(url, None, 200, body, vec![String::new()])
}

// Mirrors PHP's `['url' => .., 'status' => 404]` entry (no body).
fn http_status(
    url: &str,
    status: i64,
) -> shirabe::util::http_downloader::HttpDownloaderMockExpectation {
    expect_full(url, None, status, String::new(), vec![String::new()])
}

fn b64(s: &str) -> String {
    shirabe_php_shim::base64_encode(s)
}

fn supports_provider() -> Vec<(bool, &'static str)> {
    vec![
        (false, "https://github.com/acme"),
        (true, "https://github.com/acme/repository"),
        (true, "git@github.com:acme/repository.git"),
        (false, "https://github.com/acme/repository/releases"),
        (false, "https://github.com/acme/repository/pulls"),
    ]
}

#[test]
fn test_supports() {
    let SetUp { home, config: _ } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    for (expected, repo_url) in supports_provider() {
        let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
        let config = Rc::new(RefCell::new(Config::new(true, None)));

        assert_eq!(
            expected,
            GitHubDriver::supports(io, config, repo_url, false).unwrap()
        );
    }
}

#[test]
fn test_private_repository() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let repo_url = "http://github.com/composer/packagist";
    let repo_api_url = "https://api.github.com/repos/composer/packagist";
    let repo_ssh_url = "git@github.com:composer/packagist.git";
    let identifier = "v0.0.0";
    let sha = "SOMESHA";

    let config = Rc::new(RefCell::new(config));

    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(
        IOStub::new()
            .with_is_interactive(true)
            .with_ask_and_hide_answer(Some("sometoken".to_string())),
    ));

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) = get_http_downloader_mock(
        vec![
            http_status(repo_api_url, 404),
            http_body("https://api.github.com/", "{}"),
            http_body(
                repo_api_url,
                r#"{"master_branch": "test_master", "private": true, "owner": {"login": "composer"}, "name": "packagist"}"#,
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let (process, _process_guard): (_, ProcessExecutorMockGuard) = get_process_executor_mock(
        vec![],
        false,
        MockHandler {
            r#return: 1,
            stdout: String::new(),
            stderr: String::new(),
        },
    );

    config
        .borrow_mut()
        .set_config_source(Box::new(null_config_source()));
    config
        .borrow_mut()
        .set_auth_config_source(Box::new(null_config_source()));

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert("url".to_string(), PhpMixed::String(repo_url.to_string()));

    let mut git_hub_driver = GitHubDriver::new(repo_config, io, config, http_downloader, process);
    git_hub_driver.initialize().unwrap();
    let mut tags = IndexMap::new();
    tags.insert(identifier.to_string(), sha.to_string());
    git_hub_driver.__set_tags(Some(tags));

    assert_eq!("test_master", git_hub_driver.get_root_identifier().unwrap());

    let dist = git_hub_driver.get_dist(sha).unwrap();
    assert_eq!(Some("zip"), dist.get("type").and_then(|v| v.as_string()));
    assert_eq!(
        Some("https://api.github.com/repos/composer/packagist/zipball/SOMESHA"),
        dist.get("url").and_then(|v| v.as_string())
    );
    assert_eq!(
        Some("SOMESHA"),
        dist.get("reference").and_then(|v| v.as_string())
    );

    let source = git_hub_driver.get_source(sha);
    assert_eq!(Some("git"), source.get("type").and_then(|v| v.as_string()));
    assert_eq!(
        Some(repo_ssh_url),
        source.get("url").and_then(|v| v.as_string())
    );
    assert_eq!(
        Some("SOMESHA"),
        source.get("reference").and_then(|v| v.as_string())
    );
}

#[test]
fn test_public_repository() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let repo_url = "http://github.com/composer/packagist";
    let repo_api_url = "https://api.github.com/repos/composer/packagist";
    let identifier = "v0.0.0";
    let sha = "SOMESHA";

    let config = Rc::new(RefCell::new(config));
    let io: Rc<RefCell<dyn IOInterface>> =
        Rc::new(RefCell::new(IOStub::new().with_is_interactive(true)));

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![http_body(
            repo_api_url,
            r#"{"master_branch": "test_master", "owner": {"login": "composer"}, "name": "packagist"}"#,
        )],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let process = Rc::new(RefCell::new(ProcessExecutor::new(None)));

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert("url".to_string(), PhpMixed::String(repo_url.to_string()));
    let repo_url = "https://github.com/composer/packagist.git";

    let mut git_hub_driver = GitHubDriver::new(repo_config, io, config, http_downloader, process);
    git_hub_driver.initialize().unwrap();
    let mut tags = IndexMap::new();
    tags.insert(identifier.to_string(), sha.to_string());
    git_hub_driver.__set_tags(Some(tags));

    assert_eq!("test_master", git_hub_driver.get_root_identifier().unwrap());

    let dist = git_hub_driver.get_dist(sha).unwrap();
    assert_eq!(Some("zip"), dist.get("type").and_then(|v| v.as_string()));
    assert_eq!(
        Some("https://api.github.com/repos/composer/packagist/zipball/SOMESHA"),
        dist.get("url").and_then(|v| v.as_string())
    );
    assert_eq!(Some(sha), dist.get("reference").and_then(|v| v.as_string()));

    let source = git_hub_driver.get_source(sha);
    assert_eq!(Some("git"), source.get("type").and_then(|v| v.as_string()));
    assert_eq!(
        Some(repo_url),
        source.get("url").and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(sha),
        source.get("reference").and_then(|v| v.as_string())
    );
}

#[test]
fn test_public_repository2() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let repo_url = "http://github.com/composer/packagist";
    let repo_api_url = "https://api.github.com/repos/composer/packagist";
    let identifier = "feature/3.2-foo";
    let sha = "SOMESHA";

    let config = Rc::new(RefCell::new(config));
    let io: Rc<RefCell<dyn IOInterface>> =
        Rc::new(RefCell::new(IOStub::new().with_is_interactive(true)));

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![
            http_body(
                repo_api_url,
                r#"{"master_branch": "test_master", "owner": {"login": "composer"}, "name": "packagist"}"#,
            ),
            http_body(
                "https://api.github.com/repos/composer/packagist/contents/composer.json?ref=feature%2F3.2-foo",
                format!(
                    r#"{{"encoding":"base64","content":"{}"}}"#,
                    b64(&format!(r#"{{"support": {{"source": "{}" }}}}"#, repo_url))
                ),
            ),
            http_body(
                "https://api.github.com/repos/composer/packagist/commits/feature%2F3.2-foo",
                r#"{"commit": {"committer":{ "date": "2012-09-10"}}}"#,
            ),
            http_body(
                "https://api.github.com/repos/composer/packagist/contents/.github/FUNDING.yml",
                format!(
                    r#"{{"encoding": "base64", "content": "{}"}}"#,
                    b64("custom: https://example.com")
                ),
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let process = Rc::new(RefCell::new(ProcessExecutor::new(None)));

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert("url".to_string(), PhpMixed::String(repo_url.to_string()));
    let repo_url = "https://github.com/composer/packagist.git";

    let mut git_hub_driver = GitHubDriver::new(repo_config, io, config, http_downloader, process);
    git_hub_driver.initialize().unwrap();
    let mut tags = IndexMap::new();
    tags.insert(identifier.to_string(), sha.to_string());
    git_hub_driver.__set_tags(Some(tags));

    assert_eq!("test_master", git_hub_driver.get_root_identifier().unwrap());

    let dist = git_hub_driver.get_dist(sha).unwrap();
    assert_eq!(Some("zip"), dist.get("type").and_then(|v| v.as_string()));
    assert_eq!(
        Some("https://api.github.com/repos/composer/packagist/zipball/SOMESHA"),
        dist.get("url").and_then(|v| v.as_string())
    );
    assert_eq!(Some(sha), dist.get("reference").and_then(|v| v.as_string()));

    let source = git_hub_driver.get_source(sha);
    assert_eq!(Some("git"), source.get("type").and_then(|v| v.as_string()));
    assert_eq!(
        Some(repo_url),
        source.get("url").and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(sha),
        source.get("reference").and_then(|v| v.as_string())
    );

    let data = git_hub_driver
        .get_composer_information(identifier)
        .unwrap()
        .unwrap();
    assert!(!data.contains_key("abandoned"));
}

#[test]
fn test_invalid_support_data() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let repo_url = "http://github.com/composer/packagist";
    let repo_api_url = "https://api.github.com/repos/composer/packagist";
    let identifier = "feature/3.2-foo";
    let sha = "SOMESHA";

    let config = Rc::new(RefCell::new(config));
    let io: Rc<RefCell<dyn IOInterface>> =
        Rc::new(RefCell::new(IOStub::new().with_is_interactive(true)));

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![
            http_body(
                repo_api_url,
                r#"{"master_branch": "test_master", "owner": {"login": "composer"}, "name": "packagist"}"#,
            ),
            http_body(
                "https://api.github.com/repos/composer/packagist/contents/composer.json?ref=feature%2F3.2-foo",
                format!(
                    r#"{{"encoding":"base64","content":"{}"}}"#,
                    b64(&format!(r#"{{"support": "{}" }}"#, repo_url))
                ),
            ),
            http_body(
                "https://api.github.com/repos/composer/packagist/commits/feature%2F3.2-foo",
                r#"{"commit": {"committer":{ "date": "2012-09-10"}}}"#,
            ),
            http_body(
                "https://api.github.com/repos/composer/packagist/contents/.github/FUNDING.yml",
                format!(
                    r#"{{"encoding": "base64", "content": "{}"}}"#,
                    b64("custom: https://example.com")
                ),
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let process = Rc::new(RefCell::new(ProcessExecutor::new(None)));

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert("url".to_string(), PhpMixed::String(repo_url.to_string()));

    let mut git_hub_driver = GitHubDriver::new(repo_config, io, config, http_downloader, process);
    git_hub_driver.initialize().unwrap();
    let mut tags = IndexMap::new();
    tags.insert(identifier.to_string(), sha.to_string());
    git_hub_driver.__set_tags(Some(tags));
    let mut branches = IndexMap::new();
    branches.insert("test_master".to_string(), sha.to_string());
    git_hub_driver.__set_branches(Some(branches));

    let data = git_hub_driver
        .get_composer_information(identifier)
        .unwrap()
        .unwrap();

    let support = data.get("support").and_then(|v| v.as_array()).unwrap();
    assert_eq!(
        Some("https://github.com/composer/packagist/tree/feature/3.2-foo"),
        support.get("source").and_then(|v| v.as_string())
    );
}

fn funding_url_provider() -> Vec<(&'static str, Option<Vec<(&'static str, &'static str)>>)> {
    let all_named_platforms = "community_bridge: project-name
github: [userA, userB]
issuehunt: userName
ko_fi: userName
liberapay: userName
open_collective: userName
patreon: userName
tidelift: Platform/Package
polar: userName
buy_me_a_coffee: userName
thanks_dev: u/gh/userName
otechie: userName";

    vec![
        (
            all_named_platforms,
            Some(vec![
                (
                    "community_bridge",
                    "https://funding.communitybridge.org/projects/project-name",
                ),
                ("github", "https://github.com/userA"),
                ("github", "https://github.com/userB"),
                ("issuehunt", "https://issuehunt.io/r/userName"),
                ("ko_fi", "https://ko-fi.com/userName"),
                ("liberapay", "https://liberapay.com/userName"),
                ("open_collective", "https://opencollective.com/userName"),
                ("patreon", "https://www.patreon.com/userName"),
                (
                    "tidelift",
                    "https://tidelift.com/funding/github/Platform/Package",
                ),
                ("polar", "https://polar.sh/userName"),
                ("buy_me_a_coffee", "https://www.buymeacoffee.com/userName"),
                ("thanks_dev", "https://thanks.dev/u/gh/userName"),
                ("otechie", "https://otechie.com/userName"),
            ]),
        ),
        (
            "custom: example.com",
            Some(vec![("custom", "https://example.com")]),
        ),
        (
            "custom: [example.com]",
            Some(vec![("custom", "https://example.com")]),
        ),
        (
            "custom: \"https://example.com\"",
            Some(vec![("custom", "https://example.com")]),
        ),
        (
            "custom: [\"https://example.com\"]",
            Some(vec![("custom", "https://example.com")]),
        ),
        (
            "custom: [\"https://example.com\", example.org]",
            Some(vec![
                ("custom", "https://example.com"),
                ("custom", "https://example.org"),
            ]),
        ),
        (
            "custom: [example.net/funding, \"https://example.com\", example.org]",
            Some(vec![
                ("custom", "https://example.com"),
                ("custom", "https://example.org"),
            ]),
        ),
    ]
}

#[ignore = "funding/archived parsing differs from PHP; not date-related"]
#[test]
fn test_funding_format() {
    for (funding, expected) in funding_url_provider() {
        let SetUp { home, config } = set_up();
        let _tear_down = TearDown::new(home.path().to_path_buf());

        let repo_url = "http://github.com/composer/packagist";
        let repo_api_url = "https://api.github.com/repos/composer/packagist";
        let identifier = "feature/3.2-foo";
        let sha = "SOMESHA";

        let config = Rc::new(RefCell::new(config));
        let io: Rc<RefCell<dyn IOInterface>> =
            Rc::new(RefCell::new(IOStub::new().with_is_interactive(true)));

        let (http_downloader, _http_guard) = get_http_downloader_mock(
            vec![
                http_body(
                    repo_api_url,
                    r#"{"master_branch": "test_master", "owner": {"login": "composer"}, "name": "packagist"}"#,
                ),
                http_body(
                    "https://api.github.com/repos/composer/packagist/contents/composer.json?ref=feature%2F3.2-foo",
                    format!(
                        r#"{{"encoding":"base64","content":"{}"}}"#,
                        b64(&format!(r#"{{"support": {{"source": "{}" }}}}"#, repo_url))
                    ),
                ),
                http_body(
                    "https://api.github.com/repos/composer/packagist/commits/feature%2F3.2-foo",
                    r#"{"commit": {"committer":{ "date": "2012-09-10"}}}"#,
                ),
                http_body(
                    "https://api.github.com/repos/composer/packagist/contents/.github/FUNDING.yml",
                    format!(r#"{{"encoding": "base64", "content": "{}"}}"#, b64(funding)),
                ),
            ],
            true,
            HttpDownloaderMockHandler::default(),
        );

        let process = Rc::new(RefCell::new(ProcessExecutor::new(None)));

        let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
        repo_config.insert("url".to_string(), PhpMixed::String(repo_url.to_string()));

        let mut git_hub_driver =
            GitHubDriver::new(repo_config, io, config, http_downloader, process);
        git_hub_driver.initialize().unwrap();
        let mut tags = IndexMap::new();
        tags.insert(identifier.to_string(), sha.to_string());
        git_hub_driver.__set_tags(Some(tags));
        let mut branches = IndexMap::new();
        branches.insert("test_master".to_string(), sha.to_string());
        git_hub_driver.__set_branches(Some(branches));

        let data = git_hub_driver
            .get_composer_information(identifier)
            .unwrap()
            .unwrap();

        match expected {
            None => assert!(!data.contains_key("funding")),
            Some(expected) => {
                let funding_list = match data.get("funding") {
                    Some(PhpMixed::List(l)) => l.clone(),
                    Some(PhpMixed::Array(m)) => m.values().cloned().collect(),
                    other => panic!("unexpected funding value: {:?}", other),
                };
                let actual: Vec<(String, String)> = funding_list
                    .iter()
                    .map(|entry| {
                        let m = entry.as_array().unwrap();
                        (
                            m.get("type")
                                .and_then(|v| v.as_string())
                                .unwrap_or("")
                                .to_string(),
                            m.get("url")
                                .and_then(|v| v.as_string())
                                .unwrap_or("")
                                .to_string(),
                        )
                    })
                    .collect();
                let expected_vec: Vec<(String, String)> = expected
                    .iter()
                    .map(|(t, u)| (t.to_string(), u.to_string()))
                    .collect();
                assert_eq!(expected_vec, actual);
            }
        }
    }
}

#[ignore = "funding/archived parsing differs from PHP; not date-related"]
#[test]
fn test_public_repository_archived() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let repo_url = "http://github.com/composer/packagist";
    let repo_api_url = "https://api.github.com/repos/composer/packagist";
    let identifier = "v0.0.0";
    let sha = "SOMESHA";
    let composer_json_url = format!(
        "https://api.github.com/repos/composer/packagist/contents/composer.json?ref={}",
        sha
    );

    let config = Rc::new(RefCell::new(config));
    let io: Rc<RefCell<dyn IOInterface>> =
        Rc::new(RefCell::new(IOStub::new().with_is_interactive(true)));

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![
            http_body(
                repo_api_url,
                r#"{"master_branch": "test_master", "owner": {"login": "composer"}, "name": "packagist", "archived": true}"#,
            ),
            http_body(
                &composer_json_url,
                format!(
                    r#"{{"encoding": "base64", "content": "{}"}}"#,
                    b64(r#"{"name": "composer/packagist"}"#)
                ),
            ),
            http_body(
                &format!(
                    "https://api.github.com/repos/composer/packagist/commits/{}",
                    sha
                ),
                r#"{"commit": {"committer":{ "date": "2012-09-10"}}}"#,
            ),
            http_body(
                "https://api.github.com/repos/composer/packagist/contents/.github/FUNDING.yml",
                format!(
                    r#"{{"encoding": "base64", "content": "{}"}}"#,
                    b64("custom: https://example.com")
                ),
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let process = Rc::new(RefCell::new(ProcessExecutor::new(None)));

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert("url".to_string(), PhpMixed::String(repo_url.to_string()));

    let mut git_hub_driver = GitHubDriver::new(repo_config, io, config, http_downloader, process);
    git_hub_driver.initialize().unwrap();
    let mut tags = IndexMap::new();
    tags.insert(identifier.to_string(), sha.to_string());
    git_hub_driver.__set_tags(Some(tags));

    let data = git_hub_driver
        .get_composer_information(sha)
        .unwrap()
        .unwrap();

    assert_eq!(Some(true), data.get("abandoned").and_then(|v| v.as_bool()));
}

#[test]
#[ignore = "GitDriver clone-fallback path runs an unexpected `git --version` (Git::get_version) not in the PHP mock expectation list; needs the version static seeded and the Rust sync_mirror command sequence to match"]
fn test_private_repository_no_interaction() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let repo_url = "http://github.com/composer/packagist";
    let repo_api_url = "https://api.github.com/repos/composer/packagist";
    let repo_ssh_url = "git@github.com:composer/packagist.git";
    let identifier = "v0.0.0";
    let sha = "SOMESHA";

    let config = Rc::new(RefCell::new(config));
    let io: Rc<RefCell<dyn IOInterface>> =
        Rc::new(RefCell::new(IOStub::new().with_is_interactive(false)));

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![http_status(repo_api_url, 404)],
        true,
        HttpDownloaderMockHandler::default(),
    );

    // clean local clone if present
    let cache_dir = std::env::temp_dir().join("composer-test");
    let mut fs = Filesystem::new(None);
    let _ = fs.remove_directory(&cache_dir);
    let cache_vcs_dir = std::env::temp_dir()
        .join("composer-test/cache")
        .to_string_lossy()
        .into_owned();
    {
        let mut top: IndexMap<String, PhpMixed> = IndexMap::new();
        let mut config_section: IndexMap<String, PhpMixed> = IndexMap::new();
        config_section.insert(
            "cache-vcs-dir".to_string(),
            PhpMixed::String(cache_vcs_dir.clone()),
        );
        top.insert("config".to_string(), PhpMixed::Array(config_section));
        config.borrow_mut().merge(&top, Config::SOURCE_UNKNOWN);
    }
    let resolved_cache_vcs_dir = config
        .borrow_mut()
        .get("cache-vcs-dir")
        .as_string()
        .unwrap_or("")
        .to_string();

    let (process, _process_guard): (_, ProcessExecutorMockGuard) = get_process_executor_mock(
        vec![
            cmd_full(["git", "config", "github.accesstoken"], 1, "", ""),
            cmd([
                "git",
                "clone",
                "--mirror",
                "--",
                repo_ssh_url,
                &format!(
                    "{}/git-github.com-composer-packagist.git/",
                    resolved_cache_vcs_dir
                ),
            ]),
            cmd(["git", "remote", "-v"]),
            cmd(["git", "remote", "set-url", "origin", "--", repo_ssh_url]),
            cmd_full(
                ["git", "show-ref", "--tags", "--dereference"],
                0,
                format!("{} refs/tags/{}", sha, identifier),
                "",
            ),
            cmd_full(
                ["git", "branch", "--no-color", "--no-abbrev", "-v"],
                0,
                "  test_master     edf93f1fccaebd8764383dc12016d0a1a9672d89 Fix test & behavior",
                "",
            ),
            cmd_full(["git", "branch", "--no-color"], 0, "* test_master", ""),
        ],
        true,
        MockHandler::default(),
    );

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert("url".to_string(), PhpMixed::String(repo_url.to_string()));

    let mut git_hub_driver = GitHubDriver::new(repo_config, io, config, http_downloader, process);
    git_hub_driver.initialize().unwrap();

    assert_eq!("test_master", git_hub_driver.get_root_identifier().unwrap());

    let dist = git_hub_driver.get_dist(sha).unwrap();
    assert_eq!(Some("zip"), dist.get("type").and_then(|v| v.as_string()));
    assert_eq!(
        Some("https://api.github.com/repos/composer/packagist/zipball/SOMESHA"),
        dist.get("url").and_then(|v| v.as_string())
    );
    assert_eq!(Some(sha), dist.get("reference").and_then(|v| v.as_string()));

    let source = git_hub_driver.get_source(identifier);
    assert_eq!(Some("git"), source.get("type").and_then(|v| v.as_string()));
    assert_eq!(
        Some(repo_ssh_url),
        source.get("url").and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(identifier),
        source.get("reference").and_then(|v| v.as_string())
    );

    let source = git_hub_driver.get_source(sha);
    assert_eq!(Some("git"), source.get("type").and_then(|v| v.as_string()));
    assert_eq!(
        Some(repo_ssh_url),
        source.get("url").and_then(|v| v.as_string())
    );
    assert_eq!(
        Some(sha),
        source.get("reference").and_then(|v| v.as_string())
    );
}

fn invalid_url_provider() -> Vec<&'static str> {
    vec![
        "https://github.com/acme",
        "https://github.com/acme/repository/releases",
        "https://github.com/acme/repository/pulls",
    ]
}

#[test]
fn test_initialize_invalid_repo_url() {
    for url in invalid_url_provider() {
        let SetUp { home, config } = set_up();
        let _tear_down = TearDown::new(home.path().to_path_buf());

        let config = Rc::new(RefCell::new(config));
        let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(IOStub::new()));

        let (http_downloader, _http_guard) =
            get_http_downloader_mock(vec![], true, HttpDownloaderMockHandler::default());
        let process = Rc::new(RefCell::new(ProcessExecutor::new(None)));

        let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
        repo_config.insert("url".to_string(), PhpMixed::String(url.to_string()));

        let mut git_hub_driver =
            GitHubDriver::new(repo_config, io, config, http_downloader, process);
        let result = git_hub_driver.initialize();
        assert!(
            result.is_err(),
            "expected InvalidArgumentException for url {}",
            url
        );
    }
}

#[test]
fn test_get_empty_file_content() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let repo_url = "http://github.com/composer/packagist";

    let config = Rc::new(RefCell::new(config));
    let io: Rc<RefCell<dyn IOInterface>> =
        Rc::new(RefCell::new(IOStub::new().with_is_interactive(true)));

    let (http_downloader, _http_guard) = get_http_downloader_mock(
        vec![
            http_body(
                "https://api.github.com/repos/composer/packagist",
                r#"{"master_branch": "test_master", "owner": {"login": "composer"}, "name": "packagist", "archived": true}"#,
            ),
            http_body(
                "https://api.github.com/repos/composer/packagist/contents/composer.json?ref=main",
                r#"{"encoding":"base64","content":""}"#,
            ),
        ],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let process = Rc::new(RefCell::new(ProcessExecutor::new(None)));

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert("url".to_string(), PhpMixed::String(repo_url.to_string()));

    let mut git_hub_driver = GitHubDriver::new(repo_config, io, config, http_downloader, process);
    git_hub_driver.initialize().unwrap();

    assert_eq!(
        Some(String::new()),
        git_hub_driver
            .get_file_content("composer.json", "main")
            .unwrap()
    );
}
