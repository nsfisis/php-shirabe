//! ref: composer/tests/Composer/Test/Package/Loader/RootPackageLoaderTest.php

// These build a RootPackageLoader with a mocked RepositoryManager and either a mocked
// ProcessExecutor / VersionGuesser or require constraints whose parsing goes through a
// look-around regex the regex crate cannot compile.

#[test]
#[ignore = "RootPackageLoader::load parses require constraints via a look-around regex; mocks RepositoryManager"]
fn test_stability_flags_parsing() {
    todo!()
}

#[test]
#[ignore = "mocks RepositoryManager and a ProcessExecutor returning a non-zero git result"]
fn test_no_version_is_visible_in_pretty_version() {
    todo!()
}

#[test]
#[ignore = "mocks RepositoryManager and a VersionGuesser returning a fixed guessed version"]
fn test_pretty_version_for_root_package_in_version_branch() {
    todo!()
}

#[test]
#[ignore = "mocks RepositoryManager and a ProcessExecutor feeding git branch output"]
fn test_feature_branch_pretty_version() {
    todo!()
}

#[test]
#[ignore = "mocks RepositoryManager and a ProcessExecutor feeding git branch output"]
fn test_non_feature_branch_pretty_version() {
    todo!()
}
