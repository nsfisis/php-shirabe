//! ref: composer/tests/Composer/Test/Repository/VcsRepositoryTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::dumper::ArrayDumper;
use shirabe::repository::RepositoryInterface;
use shirabe::repository::VcsRepository;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe_php_shim::PhpMixed;

struct SetUp {
    composer_home: std::path::PathBuf,
    git_repo: std::path::PathBuf,
}

fn set_up() -> SetUp {
    // setUp lazily runs initialize(), which shells out to git to build a fixture repository
    // on disk; the ExecutableFinder/ProcessExecutor-driven setup and the markTestSkipped
    // skip path are not ported.
    todo!()
}

fn tear_down(composer_home: &std::path::Path, git_repo: &std::path::Path) {
    let mut fs = Filesystem::new(None);
    fs.remove_directory(composer_home).unwrap();
    fs.remove_directory(git_repo).unwrap();
}

struct TearDown {
    composer_home: std::path::PathBuf,
    git_repo: std::path::PathBuf,
}

impl TearDown {
    fn new(composer_home: std::path::PathBuf, git_repo: std::path::PathBuf) -> Self {
        TearDown {
            composer_home,
            git_repo,
        }
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.composer_home, &self.git_repo);
    }
}

// testLoadVersions initialises a real git repository on disk and drives a VcsRepository over
// it, then asserts the loaded package versions; the git fixture setup and constraint parsing
// (look-around regex) are not ported. In addition, VcsRepository does not implement
// RepositoryInterface and keeps its inner ArrayRepository pub(crate), so getPackages() (inherited
// from ArrayRepository in PHP) is not reachable from the test crate.
#[test]
#[ignore = "VcsRepository does not expose get_packages() to the test crate; git fixture setup not ported"]
fn test_load_versions() {
    todo!()
}
