//! ref: composer/tests/Composer/Test/Package/Version/VersionGuesserTest.php

use std::cell::RefCell;
use std::rc::Rc;

use shirabe::config::Config;
use shirabe::package::version::{VersionGuesser, VersionParser};
use shirabe::util::platform::Platform;
use shirabe::util::process_executor::ProcessExecutor;

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
#[ignore = "requires getProcessExecutorMock with expects() command expectations; no ProcessExecutorMock mocking infrastructure exists"]
#[test]
fn test_hg_guess_version_returns_data() {
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations; no ProcessExecutorMock mocking infrastructure exists"]
#[test]
fn test_guess_version_returns_data() {
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations; no ProcessExecutorMock mocking infrastructure exists"]
#[test]
fn test_guess_version_does_not_see_custom_default_branch_as_non_feature_branch() {
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations; no ProcessExecutorMock mocking infrastructure exists"]
#[test]
fn test_guess_version_reads_and_respects_non_feature_branches_configuration_for_arbitrary_naming() {
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations; no ProcessExecutorMock mocking infrastructure exists"]
#[test]
fn test_guess_version_reads_and_respects_non_feature_branches_configuration_for_arbitrary_naming_regex()
 {
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations; no ProcessExecutorMock mocking infrastructure exists"]
#[test]
fn test_guess_version_reads_and_respects_non_feature_branches_configuration_for_arbitrary_naming_when_on_non_feature_branch()
 {
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations; no ProcessExecutorMock mocking infrastructure exists"]
#[test]
fn test_detached_head_becomes_dev_hash() {
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations; no ProcessExecutorMock mocking infrastructure exists"]
#[test]
fn test_detached_fetch_head_becomes_dev_hash_git2() {
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations; no ProcessExecutorMock mocking infrastructure exists"]
#[test]
fn test_detached_commit_head_becomes_dev_hash_git2() {
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations; no ProcessExecutorMock mocking infrastructure exists"]
#[test]
fn test_tag_becomes_version() {
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations; no ProcessExecutorMock mocking infrastructure exists"]
#[test]
fn test_tag_becomes_pretty_version() {
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations; no ProcessExecutorMock mocking infrastructure exists"]
#[test]
fn test_invalid_tag_becomes_version() {
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations; no ProcessExecutorMock mocking infrastructure exists"]
#[test]
fn test_numeric_branches_show_nicely() {
    todo!()
}

#[ignore = "requires getProcessExecutorMock with expects() command expectations; no ProcessExecutorMock mocking infrastructure exists"]
#[test]
fn test_remote_branches_are_selected() {
    todo!()
}

#[ignore]
#[test]
fn test_get_root_version_from_env() {
    // @dataProvider rootEnvVersionsProvider
    let root_env_versions: Vec<(&str, &str)> = vec![
        ("1.0-dev", "1.0.x-dev"),
        ("1.0.x-dev", "1.0.x-dev"),
        ("1-dev", "1.x-dev"),
        ("1.x-dev", "1.x-dev"),
        ("1.0.0", "1.0.0"),
    ];

    for (env, expected_version) in root_env_versions {
        Platform::put_env("COMPOSER_ROOT_VERSION", env);
        let config = Rc::new(RefCell::new(Config::new(true, None)));
        let process = Rc::new(RefCell::new(ProcessExecutor::new(None)));
        let guesser = VersionGuesser::new(config, process, VersionParser::new(), None);
        assert_eq!(
            expected_version,
            guesser.get_root_version_from_env().unwrap()
        );
        Platform::clear_env("COMPOSER_ROOT_VERSION");
    }
}
