//! ref: composer/tests/Composer/Test/Repository/VcsRepositoryTest.php

use shirabe::util::filesystem::Filesystem;

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
// (look-around regex) are not ported.
#[test]
#[ignore = "not yet ported (initialises a git repo on disk and loads versions; constraint parsing uses a look-around regex)"]
fn test_load_versions() {
    let SetUp {
        composer_home,
        git_repo,
    } = set_up();
    let _tear_down = TearDown::new(composer_home, git_repo);
    todo!()
}
