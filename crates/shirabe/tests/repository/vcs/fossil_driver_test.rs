//! ref: composer/tests/Composer/Test/Repository/Vcs/FossilDriverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::FossilDriver;
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
        ("http://fossil.kd2.org/kd2fw/", true),
        (
            "https://chiselapp.com/user/rkeene/repository/flint/index",
            true,
        ),
        ("ssh://fossil.kd2.org/kd2fw.fossil", true),
    ]
}

#[test]
fn test_support() {
    for (url, assertion) in support_provider() {
        let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
        let config = Rc::new(RefCell::new(Config::new(true, None)));
        let result = FossilDriver::supports(io, config, url, false).unwrap();
        assert_eq!(assertion, result);
    }
}
