//! ref: composer/tests/Composer/Test/Repository/Vcs/SvnDriverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::SvnDriver;
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

fn support_provider() -> Vec<(&'static str, bool)> {
    vec![
        ("http://svn.apache.org", true),
        ("https://svn.sf.net", true),
        ("svn://example.org", true),
        ("svn+ssh://example.org", true),
    ]
}

#[test]
fn test_support() {
    for (url, assertion) in support_provider() {
        let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
        let config = Rc::new(RefCell::new(Config::new(true, None)));

        assert_eq!(
            assertion,
            SvnDriver::supports(io, config, url, false).unwrap()
        );
    }
}

// Constructs an SvnDriver and runs an svn command via a mocked ProcessExecutor; mocking is
// not available here.
#[test]
#[ignore = "constructs an SvnDriver and mocks a ProcessExecutor for the svn invocation"]
fn test_wrong_credentials_in_url() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    let _ = &config;
    todo!()
}
