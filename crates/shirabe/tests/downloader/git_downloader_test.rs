//! ref: composer/tests/Composer/Test/Downloader/GitDownloaderTest.php

// These construct a GitDownloader with a mocked IO/Config and a mocked ProcessExecutor to
// feed git command output; mocking is not available, and a real HttpDownloader reaches
// curl_multi_init (todo!()).
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks ProcessExecutor/IO and needs an HttpDownloader (curl_multi_init todo!())"]
        fn $name() {
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
