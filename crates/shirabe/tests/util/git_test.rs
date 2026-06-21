//! ref: composer/tests/Composer/Test/Util/GitTest.php

// These mock IO/Config/ProcessExecutor to drive Git::runCommand and mirror syncing; mocking
// is not available here.

#[allow(dead_code)]
fn set_up() {
    // Builds mocked IO/Config/ProcessExecutor/Filesystem and a real Git; mocking is not available.
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/ProcessExecutor to drive Git; mocking is not available"]
fn test_run_command_public_git_hub_repository_not_initial_clone() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/ProcessExecutor to drive Git; mocking is not available"]
fn test_run_command_private_git_hub_repository_not_initial_clone_not_interactive_without_authentication()
 {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/ProcessExecutor to drive Git; mocking is not available"]
fn test_run_command_private_git_hub_repository_not_initial_clone_not_interactive_with_authentication()
 {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/ProcessExecutor to drive Git; mocking is not available"]
fn test_run_command_private_bitbucket_repository_not_initial_clone_not_interactive_with_authentication()
 {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/ProcessExecutor to drive Git; mocking is not available"]
fn test_run_command_private_bitbucket_repository_not_initial_clone_interactive_with_oauth() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/ProcessExecutor to drive Git; mocking is not available"]
fn test_sync_mirror_sanitizes_url_after_initial_clone() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/ProcessExecutor to drive Git; mocking is not available"]
fn test_sync_mirror_sanitizes_url_even_after_failed_update() {
    todo!()
}
