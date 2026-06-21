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
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks ProcessExecutor/IO and needs an HttpDownloader (curl_multi_init todo!())"]
        fn $name() {
            let working_dir = set_up();
            let _tear_down = TearDown::new(working_dir.path().to_path_buf());
            let _ = &working_dir;
            todo!()
        }
    };
}

stub!(test_download_for_package_without_source_reference);
stub!(test_download);
stub!(test_download_with_cache);
stub!(test_download_uses_various_protocols_and_sets_push_url_for_github);
stub!(test_download_and_set_push_url_use_custom_various_protocols_for_github);
stub!(test_download_throws_runtime_exception_if_git_command_fails);
stub!(test_updatefor_package_without_source_reference);
stub!(test_update);
stub!(test_update_with_new_repo_url);
stub!(test_update_throws_runtime_exception_if_git_command_fails);
stub!(test_update_doesnt_throws_runtime_exception_if_git_command_fails_at_first_but_is_able_to_recover);
stub!(test_downgrade_shows_appropriate_message);
stub!(test_not_using_downgrading_with_references);
stub!(test_remove);
stub!(test_get_installation_source);
