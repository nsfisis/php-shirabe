//! ref: composer/tests/Composer/Test/Util/GitTest.php

// These mock IO/Config/ProcessExecutor to drive Git::runCommand and mirror syncing; mocking
// is not available here.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks IO/Config/ProcessExecutor to drive Git; mocking is not available"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_run_command_public_git_hub_repository_not_initial_clone);
stub!(test_run_command_private_git_hub_repository_not_initial_clone_not_interactive_without_authentication);
stub!(test_run_command_private_git_hub_repository_not_initial_clone_not_interactive_with_authentication);
stub!(test_run_command_private_bitbucket_repository_not_initial_clone_not_interactive_with_authentication);
stub!(test_run_command_private_bitbucket_repository_not_initial_clone_interactive_with_oauth);
stub!(test_sync_mirror_sanitizes_url_after_initial_clone);
stub!(test_sync_mirror_sanitizes_url_even_after_failed_update);
