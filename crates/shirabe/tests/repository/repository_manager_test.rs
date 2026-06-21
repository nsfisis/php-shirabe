//! ref: composer/tests/Composer/Test/Repository/RepositoryManagerTest.php

// These construct a RepositoryManager (which builds an HttpDownloader reaching
// curl_multi_init, todo!()) with a mocked IO/Config/EventDispatcher and exercise repo
// creation/prepending/wrapping.

use shirabe::util::filesystem::Filesystem;
use tempfile::TempDir;

struct SetUp {
    tmpdir: TempDir,
}

fn set_up() -> SetUp {
    let tmpdir = TempDir::new().unwrap();
    SetUp { tmpdir }
}

fn tear_down(tmpdir: &std::path::Path) {
    if tmpdir.is_dir() {
        let mut fs = Filesystem::new(None);
        fs.remove_directory(tmpdir).unwrap();
    }
}

struct TearDown {
    tmpdir: std::path::PathBuf,
}

impl TearDown {
    fn new(tmpdir: std::path::PathBuf) -> Self {
        TearDown { tmpdir }
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.tmpdir);
    }
}

#[test]
#[ignore = "RepositoryManager::new builds an HttpDownloader (curl_multi_init todo!()) and mocks IO/Config"]
fn test_prepend() {
    let SetUp { tmpdir } = set_up();
    let _tear_down = TearDown::new(tmpdir.path().to_path_buf());
    todo!()
}

#[test]
#[ignore = "RepositoryManager::new builds an HttpDownloader (curl_multi_init todo!()) and mocks IO/Config"]
fn test_repo_creation() {
    let SetUp { tmpdir } = set_up();
    let _tear_down = TearDown::new(tmpdir.path().to_path_buf());
    todo!()
}

#[test]
#[ignore = "RepositoryManager::new builds an HttpDownloader (curl_multi_init todo!()) and mocks IO/Config"]
fn test_invalid_repo_creation_throws() {
    let SetUp { tmpdir } = set_up();
    let _tear_down = TearDown::new(tmpdir.path().to_path_buf());
    todo!()
}

#[test]
#[ignore = "RepositoryManager::new builds an HttpDownloader (curl_multi_init todo!()) and mocks IO/Config"]
fn test_filter_repo_wrapping() {
    let SetUp { tmpdir } = set_up();
    let _tear_down = TearDown::new(tmpdir.path().to_path_buf());
    todo!()
}
