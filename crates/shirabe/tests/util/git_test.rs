//! ref: composer/tests/Composer/Test/Util/GitTest.php

// These mock IO/Config/ProcessExecutor to drive Git::runCommand and mirror syncing; mocking
// is not available here.

#[allow(dead_code)]
fn set_up() {
    // Builds mocked IO/Config/ProcessExecutor/Filesystem and a real Git; mocking is not available.
    todo!()
}

#[test]
#[ignore = "requires mocked Config (getMockBuilder Config) and getProcessExecutorMock with expects() command expectations; no mocking infrastructure exists"]
fn test_run_command_public_git_hub_repository_not_initial_clone() {
    todo!()
}

#[test]
#[ignore = "requires mocked Config and getProcessExecutorMock with expects() command expectations; no mocking infrastructure exists"]
fn test_run_command_private_git_hub_repository_not_initial_clone_not_interactive_without_authentication()
 {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock and mocked IO (getMockBuilder IOInterface with hasAuthentication/getAuthentication/isInteractive expectations); no mocking infrastructure exists"]
fn test_run_command_private_git_hub_repository_not_initial_clone_not_interactive_with_authentication()
 {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock and mocked IO/Config (getMockBuilder with hasAuthentication/getAuthentication/isInteractive expectations); no mocking infrastructure exists"]
fn test_run_command_private_bitbucket_repository_not_initial_clone_not_interactive_with_authentication()
 {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock, getHttpDownloaderMock and mocked IO/Config with askConfirmation/askAndHideAnswer/setAuthentication expectations; no mocking infrastructure exists"]
fn test_run_command_private_bitbucket_repository_not_initial_clone_interactive_with_oauth() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock with expects() command expectations and mocked Config/Filesystem (getMockBuilder removeDirectory); no mocking infrastructure exists"]
fn test_sync_mirror_sanitizes_url_after_initial_clone() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock with expects() command expectations and mocked Config; no mocking infrastructure exists"]
fn test_sync_mirror_sanitizes_url_even_after_failed_update() {
    todo!()
}
