//! ref: composer/tests/Composer/Test/Package/Version/VersionGuesserTest.php

// These drive VersionGuesser with a mocked ProcessExecutor feeding git/hg command output;
// mocking is not available here.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks a ProcessExecutor feeding git/hg output to drive VersionGuesser; mocking is not available"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_hg_guess_version_returns_data);
stub!(test_guess_version_returns_data);
stub!(test_guess_version_does_not_see_custom_default_branch_as_non_feature_branch);
stub!(test_guess_version_reads_and_respects_non_feature_branches_configuration_for_arbitrary_naming);
stub!(test_guess_version_reads_and_respects_non_feature_branches_configuration_for_arbitrary_naming_regex);
stub!(test_guess_version_reads_and_respects_non_feature_branches_configuration_for_arbitrary_naming_when_on_non_feature_branch);
stub!(test_detached_head_becomes_dev_hash);
stub!(test_detached_fetch_head_becomes_dev_hash_git2);
stub!(test_detached_commit_head_becomes_dev_hash_git2);
stub!(test_tag_becomes_version);
stub!(test_tag_becomes_pretty_version);
stub!(test_invalid_tag_becomes_version);
stub!(test_numeric_branches_show_nicely);
stub!(test_remote_branches_are_selected);
stub!(test_get_root_version_from_env);
