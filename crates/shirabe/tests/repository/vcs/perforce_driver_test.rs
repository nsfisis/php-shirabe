//! ref: composer/tests/Composer/Test/Repository/Vcs/PerforceDriverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::PerforceDriver;
use shirabe::util::filesystem::Filesystem;
use shirabe_php_shim::PhpMixed;
use tempfile::TempDir;

const TEST_URL: &str = "TEST_PERFORCE_URL";
const TEST_DEPOT: &str = "TEST_DEPOT_CONFIG";
const TEST_BRANCH: &str = "TEST_BRANCH_CONFIG";

struct SetUp {
    test_path: TempDir,
    config: Config,
    repo_config: IndexMap<String, PhpMixed>,
    // The IOInterface, ProcessExecutor, HttpDownloader and Perforce mocks, the
    // driver instance and the reflection-based perforce override are not ported.
    io: (),
    process: (),
    http_downloader: (),
    perforce: (),
    driver: (),
}

fn get_test_config(test_path: &std::path::Path) -> Config {
    let mut config = Config::new(true, None);
    let mut top: IndexMap<String, PhpMixed> = IndexMap::new();
    let mut config_section: IndexMap<String, PhpMixed> = IndexMap::new();
    config_section.insert(
        "home".to_string(),
        PhpMixed::String(test_path.to_string_lossy().into_owned()),
    );
    top.insert("config".to_string(), PhpMixed::Array(config_section));
    config.merge(&top, Config::SOURCE_UNKNOWN);

    config
}

fn set_up() -> SetUp {
    let test_path = TempDir::new().unwrap();
    let config = get_test_config(test_path.path());
    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert("url".to_string(), PhpMixed::String(TEST_URL.to_string()));
    repo_config.insert(
        "depot".to_string(),
        PhpMixed::String(TEST_DEPOT.to_string()),
    );
    repo_config.insert(
        "branch".to_string(),
        PhpMixed::String(TEST_BRANCH.to_string()),
    );

    let io = ();
    let process = ();
    let http_downloader = ();
    let perforce = ();
    // The driver construction and overrideDriverInternalPerforce (reflection) are not ported.
    let driver = ();

    SetUp {
        test_path,
        config,
        repo_config,
        io,
        process,
        http_downloader,
        perforce,
        driver,
    }
}

fn tear_down(test_path: &std::path::Path) {
    // cleanup directory under test path
    let mut fs = Filesystem::new(None);
    fs.remove_directory(test_path).unwrap();
}

struct TearDown {
    test_path: std::path::PathBuf,
}

impl TearDown {
    fn new(test_path: std::path::PathBuf) -> Self {
        TearDown { test_path }
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.test_path);
    }
}

#[test]
fn test_supports_returns_false_no_deep_check() {
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let config = Rc::new(RefCell::new(Config::new(true, None)));

    assert!(!PerforceDriver::supports(io, config, "existing.url", false).unwrap());
}

// The remaining cases mock Perforce, the repository config and IO to drive initialization,
// composer-file detection and cleanup; mocking is not available here.
#[test]
#[ignore = "mocks Perforce/repository/IO; mocking is not available"]
fn test_initialize_captures_variables_from_repo_config() {
    let SetUp {
        test_path,
        config,
        repo_config,
        io,
        process,
        http_downloader,
        perforce,
        driver,
    } = set_up();
    let _tear_down = TearDown::new(test_path.path().to_path_buf());
    let _ = (
        &config,
        &repo_config,
        &io,
        &process,
        &http_downloader,
        &perforce,
        &driver,
    );
    todo!()
}

#[test]
#[ignore = "mocks Perforce/repository/IO; mocking is not available"]
fn test_initialize_logs_in_and_connects_client() {
    let SetUp {
        test_path,
        config,
        repo_config,
        io,
        process,
        http_downloader,
        perforce,
        driver,
    } = set_up();
    let _tear_down = TearDown::new(test_path.path().to_path_buf());
    let _ = (
        &config,
        &repo_config,
        &io,
        &process,
        &http_downloader,
        &perforce,
        &driver,
    );
    todo!()
}

#[test]
#[ignore = "mocks Perforce/repository/IO; mocking is not available"]
fn test_has_composer_file_returns_false_on_no_composer_file() {
    let SetUp {
        test_path,
        config,
        repo_config,
        io,
        process,
        http_downloader,
        perforce,
        driver,
    } = set_up();
    let _tear_down = TearDown::new(test_path.path().to_path_buf());
    let _ = (
        &config,
        &repo_config,
        &io,
        &process,
        &http_downloader,
        &perforce,
        &driver,
    );
    todo!()
}

#[test]
#[ignore = "mocks Perforce/repository/IO; mocking is not available"]
fn test_has_composer_file_returns_true_with_one_or_more_composer_files() {
    let SetUp {
        test_path,
        config,
        repo_config,
        io,
        process,
        http_downloader,
        perforce,
        driver,
    } = set_up();
    let _tear_down = TearDown::new(test_path.path().to_path_buf());
    let _ = (
        &config,
        &repo_config,
        &io,
        &process,
        &http_downloader,
        &perforce,
        &driver,
    );
    todo!()
}

#[test]
#[ignore = "mocks Perforce/repository/IO; mocking is not available"]
fn test_cleanup() {
    let SetUp {
        test_path,
        config,
        repo_config,
        io,
        process,
        http_downloader,
        perforce,
        driver,
    } = set_up();
    let _tear_down = TearDown::new(test_path.path().to_path_buf());
    let _ = (
        &config,
        &repo_config,
        &io,
        &process,
        &http_downloader,
        &perforce,
        &driver,
    );
    todo!()
}
