//! ref: composer/tests/Composer/Test/Downloader/GitDownloaderTest.php

use shirabe::util::filesystem::Filesystem;
use tempfile::TempDir;

fn set_up() -> TempDir {
    // skipIfNotExecutable('git')
    let () = todo!();
    // initGitVersion('1.0.0') resets the Composer\Util\Git static version cache via
    // reflection; the static cache is not reachable from a test here.
    #[allow(unreachable_code)]
    {
        let _fs = Filesystem::new(None);
        TempDir::new().unwrap()
    }
}

fn tear_down(working_dir: &std::path::Path) {
    if working_dir.is_dir() {
        let mut fs = Filesystem::new(None);
        fs.remove_directory(working_dir).unwrap();
    }
    // initGitVersion(false) resets the Composer\Util\Git static version cache via
    // reflection; the static cache is not reachable from a test here.
    todo!()
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

// These construct a GitDownloader with a mocked IO/Config and a mocked ProcessExecutor to
// feed git command output; mocking is not available, and a real HttpDownloader reaches
// curl_multi_init (todo!()).
#[ignore = "requires getProcessExecutorMock/getIOMock and getMockBuilder PackageInterface mock; no ProcessExecutorMock/IOMock mocking infrastructure exists"]
#[test]
fn test_download_for_package_without_source_reference() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations and getMockBuilder PackageInterface mock; no mocking infrastructure exists"]
#[test]
fn test_download() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations and initGitVersion reflection on Git::version static cache; no mocking infrastructure exists"]
#[test]
fn test_download_with_cache() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations and getMockBuilder PackageInterface mock; no mocking infrastructure exists"]
#[test]
fn test_download_uses_various_protocols_and_sets_push_url_for_github() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations and pushUrlProvider dataProvider; no mocking infrastructure exists"]
#[test]
fn test_download_and_set_push_url_use_custom_various_protocols_for_github() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations and getMockBuilder PackageInterface mock; no mocking infrastructure exists"]
#[test]
fn test_download_throws_runtime_exception_if_git_command_fails() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "requires getProcessExecutorMock/getDownloaderMock and getMockBuilder PackageInterface mock; no mocking infrastructure exists"]
#[test]
fn test_updatefor_package_without_source_reference() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations and getMockBuilder PackageInterface mock; no mocking infrastructure exists"]
#[test]
fn test_update() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations and getMockBuilder PackageInterface mock; no mocking infrastructure exists"]
#[test]
fn test_update_with_new_repo_url() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations and getMockBuilder PackageInterface mock; no mocking infrastructure exists"]
#[test]
fn test_update_throws_runtime_exception_if_git_command_fails() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations and getMockBuilder PackageInterface mock; no mocking infrastructure exists"]
#[test]
fn test_update_doesnt_throws_runtime_exception_if_git_command_fails_at_first_but_is_able_to_recover()
 {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "requires getIOMock with expects() output expectations, getProcessExecutorMock and getMockBuilder PackageInterface mock; no mocking infrastructure exists"]
#[test]
fn test_downgrade_shows_appropriate_message() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "requires getIOMock with expects() output expectations, getProcessExecutorMock and getMockBuilder PackageInterface mock; no mocking infrastructure exists"]
#[test]
fn test_not_using_downgrading_with_references() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "requires getProcessExecutorMock, getMockBuilder Filesystem mock with removeDirectoryAsync expectation and PackageInterface mock; no mocking infrastructure exists"]
#[test]
fn test_remove() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}

#[ignore = "requires getDownloaderMock which mocks IOInterface/Filesystem via getMockBuilder; no mocking infrastructure exists"]
#[test]
fn test_get_installation_source() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;
    todo!()
}
