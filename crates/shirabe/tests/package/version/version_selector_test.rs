//! ref: composer/tests/Composer/Test/Package/Version/VersionSelectorTest.php

// VersionSelector ranks candidate packages whose versions/constraints are parsed through a
// look-around regex the regex crate cannot compile; the setup also mocks a repository.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "not yet ported (VersionSelector over a mocked repository; constraint parsing uses a look-around regex)"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_latest_version_is_returned);
stub!(test_latest_version_is_returned_that_matches_php_requirements);
stub!(test_latest_version_is_returned_that_matches_ext_requirements);
stub!(test_latest_version_is_returned_that_matches_platform_ext);
stub!(test_latest_version_is_returned_that_matches_composer_requirements);
stub!(test_most_stable_version_is_returned);
stub!(test_most_stable_version_is_returned_regardless_of_order);
stub!(test_highest_version_is_returned);
stub!(test_highest_version_matching_stability_is_returned);
stub!(test_most_stable_unstable_version_is_returned);
stub!(test_default_branch_alias_is_never_returned);
stub!(test_false_returned_on_no_packages);
stub!(test_find_recommended_require_version);
