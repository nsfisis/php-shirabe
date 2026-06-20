//! ref: composer/tests/Composer/Test/Repository/Vcs/GitDriverTest.php

// Every case constructs a GitDriver with a mocked ProcessExecutor (and an HttpDownloader
// that reaches curl_multi_init, todo!()) to feed git command output; mocking is not
// available here.

macro_rules! git_stub {
    ($name:ident) => {
        #[test]
        #[ignore = "constructs a GitDriver and mocks a ProcessExecutor/HttpDownloader (curl_multi_init todo!())"]
        fn $name() {
            todo!()
        }
    };
}

git_stub!(test_get_root_identifier_from_remote_local_repository);
git_stub!(test_get_root_identifier_from_remote);
git_stub!(test_get_root_identifier_from_local_with_network_disabled);
git_stub!(test_get_branches_filter_invalid_branch_names);
git_stub!(test_file_get_content_invalid_identifier);
git_stub!(test_get_change_date_invalid_identifier);
