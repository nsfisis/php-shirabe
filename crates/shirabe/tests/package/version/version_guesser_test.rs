//! ref: composer/tests/Composer/Test/Package/Version/VersionGuesserTest.php

#[allow(dead_code)]
fn set_up() {
    // Resets GitUtil's cached `version` static via ReflectionProperty; the static is not
    // exposed here and reflection-based mutation has no ported equivalent.
    todo!()
}

#[allow(dead_code)]
fn tear_down() {
    todo!()
}

#[allow(dead_code)]
struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

// These drive VersionGuesser with a mocked ProcessExecutor feeding git/hg command output;
// mocking is not available here.
#[test]
#[ignore = "mocks a ProcessExecutor feeding git/hg output to drive VersionGuesser; mocking is not available"]
fn test_hg_guess_version_returns_data() {
    todo!()
}

#[test]
#[ignore = "mocks a ProcessExecutor feeding git/hg output to drive VersionGuesser; mocking is not available"]
fn test_guess_version_returns_data() {
    todo!()
}

#[test]
#[ignore = "mocks a ProcessExecutor feeding git/hg output to drive VersionGuesser; mocking is not available"]
fn test_guess_version_does_not_see_custom_default_branch_as_non_feature_branch() {
    todo!()
}

#[test]
#[ignore = "mocks a ProcessExecutor feeding git/hg output to drive VersionGuesser; mocking is not available"]
fn test_guess_version_reads_and_respects_non_feature_branches_configuration_for_arbitrary_naming() {
    todo!()
}

#[test]
#[ignore = "mocks a ProcessExecutor feeding git/hg output to drive VersionGuesser; mocking is not available"]
fn test_guess_version_reads_and_respects_non_feature_branches_configuration_for_arbitrary_naming_regex()
 {
    todo!()
}

#[test]
#[ignore = "mocks a ProcessExecutor feeding git/hg output to drive VersionGuesser; mocking is not available"]
fn test_guess_version_reads_and_respects_non_feature_branches_configuration_for_arbitrary_naming_when_on_non_feature_branch()
 {
    todo!()
}

#[test]
#[ignore = "mocks a ProcessExecutor feeding git/hg output to drive VersionGuesser; mocking is not available"]
fn test_detached_head_becomes_dev_hash() {
    todo!()
}

#[test]
#[ignore = "mocks a ProcessExecutor feeding git/hg output to drive VersionGuesser; mocking is not available"]
fn test_detached_fetch_head_becomes_dev_hash_git2() {
    todo!()
}

#[test]
#[ignore = "mocks a ProcessExecutor feeding git/hg output to drive VersionGuesser; mocking is not available"]
fn test_detached_commit_head_becomes_dev_hash_git2() {
    todo!()
}

#[test]
#[ignore = "mocks a ProcessExecutor feeding git/hg output to drive VersionGuesser; mocking is not available"]
fn test_tag_becomes_version() {
    todo!()
}

#[test]
#[ignore = "mocks a ProcessExecutor feeding git/hg output to drive VersionGuesser; mocking is not available"]
fn test_tag_becomes_pretty_version() {
    todo!()
}

#[test]
#[ignore = "mocks a ProcessExecutor feeding git/hg output to drive VersionGuesser; mocking is not available"]
fn test_invalid_tag_becomes_version() {
    todo!()
}

#[test]
#[ignore = "mocks a ProcessExecutor feeding git/hg output to drive VersionGuesser; mocking is not available"]
fn test_numeric_branches_show_nicely() {
    todo!()
}

#[test]
#[ignore = "mocks a ProcessExecutor feeding git/hg output to drive VersionGuesser; mocking is not available"]
fn test_remote_branches_are_selected() {
    todo!()
}

#[test]
#[ignore = "mocks a ProcessExecutor feeding git/hg output to drive VersionGuesser; mocking is not available"]
fn test_get_root_version_from_env() {
    todo!()
}
