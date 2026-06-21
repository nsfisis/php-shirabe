//! ref: composer/tests/Composer/Test/Repository/Vcs/GitHubDriverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::GitHubDriver;
use shirabe::util::filesystem::Filesystem;
use shirabe_php_shim::PhpMixed;
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
#[ignore = "GitHubDriver::supports reaches non-strict in_array, which is todo!() in the php-shim"]
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

// The remaining cases construct a GitHubDriver and mock the HttpDownloader/IO to return
// GitHub API responses; mocking is not available, and a real HttpDownloader reaches
// curl_multi_init (todo!()).
#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_private_repository() {
    let SetUp { home, config: _ } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    todo!()
}

#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_public_repository() {
    let SetUp { home, config: _ } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    todo!()
}

#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_public_repository2() {
    let SetUp { home, config: _ } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    todo!()
}

#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_invalid_support_data() {
    let SetUp { home, config: _ } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    todo!()
}

#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_funding_format() {
    let SetUp { home, config: _ } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    todo!()
}

#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_public_repository_archived() {
    let SetUp { home, config: _ } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    todo!()
}

#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_private_repository_no_interaction() {
    let SetUp { home, config: _ } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    todo!()
}

#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_initialize_invalid_repo_url() {
    let SetUp { home, config: _ } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    todo!()
}

#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_get_empty_file_content() {
    let SetUp { home, config: _ } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    todo!()
}
