//! ref: composer/tests/Composer/Test/Downloader/FossilDownloaderTest.php

use shirabe::util::filesystem::Filesystem;
use tempfile::TempDir;

fn set_up() -> TempDir {
    TempDir::new().unwrap()
}

fn tear_down(working_dir: &std::path::Path) {
    if working_dir.is_dir() {
        let mut fs = Filesystem::new(None);
        fs.remove_directory(working_dir).unwrap();
    }
}

struct TearDown {
    working_dir: std::path::PathBuf,
}

impl TearDown {
    fn new(working_dir: std::path::PathBuf) -> Self {
        TearDown { working_dir }
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.working_dir);
    }
}

// Every case constructs a FossilDownloader with a mocked IO/Config and a mocked
// ProcessExecutor to feed fossil command output; a real HttpDownloader reaches
// curl_multi_init (todo!()), and ProcessExecutor mocking is not available.

#[test]
#[ignore = "needs ProcessExecutorMock and PHPUnit mocks of IOInterface/PackageInterface/Filesystem for getDownloaderMock; not available"]
fn test_install_for_package_without_source_reference() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[test]
#[ignore = "needs ProcessExecutorMock with expects() command-sequence assertions and PHPUnit PackageInterface mock; not available"]
fn test_install() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[test]
#[ignore = "needs ProcessExecutorMock and PHPUnit mocks of IOInterface/PackageInterface/Filesystem for getDownloaderMock; not available"]
fn test_updatefor_package_without_source_reference() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[test]
#[ignore = "needs ProcessExecutorMock with expects() command-sequence assertions and PHPUnit PackageInterface mock; not available"]
fn test_update() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[test]
#[ignore = "needs ProcessExecutorMock and a PHPUnit Filesystem mock asserting removeDirectoryAsync; not available"]
fn test_remove() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[test]
#[ignore = "needs ProcessExecutorMock and PHPUnit mocks of IOInterface/Filesystem for getDownloaderMock; not available"]
fn test_get_installation_source() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}
