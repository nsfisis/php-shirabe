//! ref: composer/tests/Composer/Test/Repository/Vcs/PerforceDriverTest.php

use crate::process_executor_mock::{ProcessExecutorMockGuard, get_process_executor_mock};
use bstr::BString;
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::PerforceDriver;
use shirabe::util::HttpDownloader;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::process_executor::MockHandler;
use shirabe_php_shim::PhpMixed;
use tempfile::TempDir;

const TEST_URL: &str = "TEST_PERFORCE_URL";
const TEST_DEPOT: &str = "TEST_DEPOT_CONFIG";
const TEST_BRANCH: &str = "TEST_BRANCH_CONFIG";

// A getMockBuilder('Composer\Util\Perforce') stand-in: the seam trait extracted from the
// concrete `Perforce` struct, injected via overrideDriverInternalPerforce's Rust equivalent.
mockall::mock! {
    #[derive(Debug)]
    pub Perforce {}
    impl shirabe::util::PerforceInterface for Perforce {
        fn initialize_path(&mut self, path: &std::path::Path);
        fn set_stream(&mut self, stream: &bstr::BStr);
        fn p4_login(&mut self) -> anyhow::Result<()>;
        fn check_stream(&mut self) -> bool;
        fn write_p4_client_spec(&mut self) -> anyhow::Result<()>;
        fn connect_client(&mut self) -> anyhow::Result<()>;
        fn sync_code_base(&mut self, source_reference: Option<BString>) -> anyhow::Result<()>;
        fn cleanup_client_spec(&mut self);
        fn get_commit_logs(&mut self, from_reference: &bstr::BStr, to_reference: &bstr::BStr) -> Option<BString>;
        fn get_file_content(&mut self, file: &bstr::BStr, identifier: &bstr::BStr) -> Option<BString>;
        fn get_branches(&mut self) -> IndexMap<BString, BString>;
        fn get_tags(&mut self) -> IndexMap<BString, BString>;
        fn get_user(&self) -> Option<BString>;
        fn get_composer_information(
            &mut self,
            identifier: &bstr::BStr,
        ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>>;
    }
}

struct SetUp {
    test_path: TempDir,
    config: Config,
    repo_config: IndexMap<BString, PhpMixed>,
}

fn get_test_config(test_path: &std::path::Path) -> Config {
    let mut config = Config::new(true, None);
    let mut top: IndexMap<BString, PhpMixed> = IndexMap::new();
    let mut config_section: IndexMap<BString, PhpMixed> = IndexMap::new();
    config_section.insert(
        "home".into(),
        PhpMixed::string(test_path.to_str().unwrap().to_string()),
    );
    top.insert("config".into(), PhpMixed::Array(config_section));
    config.merge(&top, Config::SOURCE_UNKNOWN);

    config
}

fn set_up() -> SetUp {
    let test_path = TempDir::new().unwrap();
    let config = get_test_config(test_path.path());
    let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
    repo_config.insert("url".into(), PhpMixed::string(TEST_URL.to_string()));
    repo_config.insert("depot".into(), PhpMixed::string(TEST_DEPOT.to_string()));
    repo_config.insert("branch".into(), PhpMixed::string(TEST_BRANCH.to_string()));

    SetUp {
        test_path,
        config,
        repo_config,
    }
}

// ref: setUp builds the driver from a mock IO, an empty ProcessExecutorMock and a mock
// HttpDownloader. The empty, non-strict process mock lets any p4 query Perforce issues resolve
// with the default (exit 0) result.
fn make_driver(
    config: Config,
    repo_config: &IndexMap<BString, PhpMixed>,
) -> (PerforceDriver, ProcessExecutorMockGuard) {
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let config = std::rc::Rc::new(std::cell::RefCell::new(config));
    let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(HttpDownloader::new(
        io.clone(),
        config.clone(),
        IndexMap::new(),
        false,
    )));
    let (process, guard) = get_process_executor_mock(vec![], false, MockHandler::default());
    let driver = PerforceDriver::new(repo_config.clone(), io, config, http_downloader, process);

    (driver, guard)
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
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(true, None)));

    assert!(!PerforceDriver::supports(io, config, bstr::BStr::new("existing.url"), false).unwrap());
}

#[test]
fn test_initialize_captures_variables_from_repo_config() {
    let SetUp {
        test_path,
        config,
        repo_config,
    } = set_up();
    let _tear_down = TearDown::new(test_path.path().to_path_buf());

    let (mut driver, _process_guard) = make_driver(config, &repo_config);
    driver.initialize().unwrap();
    assert_eq!(TEST_URL, driver.get_url());
    assert_eq!(TEST_DEPOT, driver.get_depot());
    assert_eq!(TEST_BRANCH, driver.get_branch());
}

#[test]
fn test_initialize_logs_in_and_connects_client() {
    let SetUp {
        test_path,
        config,
        repo_config,
    } = set_up();
    let _tear_down = TearDown::new(test_path.path().to_path_buf());

    let (mut driver, _process_guard) = make_driver(config, &repo_config);
    let mut perforce = MockPerforce::new();
    perforce.expect_p4_login().times(1).returning(|| Ok(()));
    perforce.expect_check_stream().times(1).returning(|| false);
    perforce
        .expect_write_p4_client_spec()
        .times(1)
        .returning(|| Ok(()));
    perforce
        .expect_connect_client()
        .times(1)
        .returning(|| Ok(()));
    driver.__override_perforce(Box::new(perforce));

    driver.initialize().unwrap();
}

#[test]
fn test_has_composer_file_returns_false_on_no_composer_file() {
    let SetUp {
        test_path,
        config,
        repo_config,
    } = set_up();
    let _tear_down = TearDown::new(test_path.path().to_path_buf());

    let identifier = "TEST_IDENTIFIER";
    let formatted_depot_path = format!("//{}/{}", TEST_DEPOT, identifier);

    let (mut driver, _process_guard) = make_driver(config, &repo_config);
    let mut perforce = MockPerforce::new();
    perforce.expect_p4_login().returning(|| Ok(()));
    perforce.expect_check_stream().returning(|| false);
    perforce.expect_write_p4_client_spec().returning(|| Ok(()));
    perforce.expect_connect_client().returning(|| Ok(()));
    perforce
        .expect_get_composer_information()
        .withf(move |id: &bstr::BStr| id == formatted_depot_path)
        .returning(|_| Ok(Some(IndexMap::new())));
    driver.__override_perforce(Box::new(perforce));

    driver.initialize().unwrap();
    let result = driver.has_composer_file(bstr::BStr::new(identifier));
    assert!(!result);
}

#[test]
fn test_has_composer_file_returns_true_with_one_or_more_composer_files() {
    let SetUp {
        test_path,
        config,
        repo_config,
    } = set_up();
    let _tear_down = TearDown::new(test_path.path().to_path_buf());

    let identifier = "TEST_IDENTIFIER";
    let formatted_depot_path = format!("//{}/{}", TEST_DEPOT, identifier);

    let (mut driver, _process_guard) = make_driver(config, &repo_config);
    let mut perforce = MockPerforce::new();
    perforce.expect_p4_login().returning(|| Ok(()));
    perforce.expect_check_stream().returning(|| false);
    perforce.expect_write_p4_client_spec().returning(|| Ok(()));
    perforce.expect_connect_client().returning(|| Ok(()));
    perforce
        .expect_get_composer_information()
        .withf(move |id: &bstr::BStr| id == formatted_depot_path)
        .returning(|_| {
            // ref: returnValue(['']) — a non-empty list with one empty-string element.
            let mut info: IndexMap<BString, PhpMixed> = IndexMap::new();
            info.insert("0".into(), PhpMixed::string(String::new()));
            Ok(Some(info))
        });
    driver.__override_perforce(Box::new(perforce));

    driver.initialize().unwrap();
    let result = driver.has_composer_file(bstr::BStr::new(identifier));
    assert!(result);
}

#[test]
fn test_cleanup() {
    let SetUp {
        test_path,
        config,
        repo_config,
    } = set_up();
    let _tear_down = TearDown::new(test_path.path().to_path_buf());

    let (mut driver, _process_guard) = make_driver(config, &repo_config);
    let mut perforce = MockPerforce::new();
    perforce
        .expect_cleanup_client_spec()
        .times(1)
        .returning(|| ());
    driver.__override_perforce(Box::new(perforce));

    driver.cleanup().unwrap();
}
