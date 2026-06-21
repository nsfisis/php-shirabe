//! ref: composer/tests/Composer/Test/Repository/Vcs/HgDriverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::HgDriver;
use shirabe::util::filesystem::Filesystem;
use shirabe_php_shim::PhpMixed;
use tempfile::TempDir;

struct SetUp {
    home: TempDir,
    config: Config,
    // The IOInterface mock is not ported.
    io: (),
}

fn set_up() -> SetUp {
    let io = ();
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

    SetUp { home, config, io }
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

// The remaining cases construct an HgDriver, which requires an HttpDownloader
// (curl_multi_init is todo!() in the php-shim) and a mocked ProcessExecutor to feed
// hg command output, neither of which is available here.
#[test]
#[ignore = "needs an HgDriver instance (HttpDownloader reaches curl_multi_init, todo!()) and a mocked ProcessExecutor"]
fn test_get_branches_filter_invalid_branch_names() {
    let SetUp { home, config, io } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    let _ = (&config, &io);
    todo!()
}

#[test]
#[ignore = "needs an HgDriver instance (HttpDownloader reaches curl_multi_init, todo!()) and a mocked ProcessExecutor"]
fn test_file_get_content_invalid_identifier() {
    let SetUp { home, config, io } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    let _ = (&config, &io);
    todo!()
}

#[test]
#[ignore = "needs an HgDriver instance (HttpDownloader reaches curl_multi_init, todo!()) and a mocked ProcessExecutor"]
fn test_get_change_date_invalid_identifier() {
    let SetUp { home, config, io } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    let _ = (&config, &io);
    todo!()
}
