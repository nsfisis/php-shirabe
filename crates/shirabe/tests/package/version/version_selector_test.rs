//! ref: composer/tests/Composer/Test/Package/Version/VersionSelectorTest.php

// VersionSelector ranks candidate packages whose versions/constraints are parsed through a
// look-around regex the regex crate cannot compile; the setup also mocks a repository.
#[test]
#[ignore = "requires mocking RepositorySet::find_packages to return fixed package objects; RepositorySet is a concrete struct with no injectable/overridable find_packages"]
fn test_latest_version_is_returned() {
    todo!()
}

#[test]
#[ignore = "requires mocking RepositorySet::find_packages to return fixed package objects; RepositorySet is a concrete struct with no injectable/overridable find_packages"]
fn test_latest_version_is_returned_that_matches_php_requirements() {
    todo!()
}

#[test]
#[ignore = "requires mocking RepositorySet::find_packages to return fixed package objects; RepositorySet is a concrete struct with no injectable/overridable find_packages"]
fn test_latest_version_is_returned_that_matches_ext_requirements() {
    todo!()
}

#[test]
#[ignore = "requires mocking RepositorySet::find_packages to return fixed package objects; RepositorySet is a concrete struct with no injectable/overridable find_packages"]
fn test_latest_version_is_returned_that_matches_platform_ext() {
    todo!()
}

#[test]
#[ignore = "requires mocking RepositorySet::find_packages to return fixed package objects; RepositorySet is a concrete struct with no injectable/overridable find_packages"]
fn test_latest_version_is_returned_that_matches_composer_requirements() {
    todo!()
}

#[test]
#[ignore = "requires mocking RepositorySet::find_packages to return fixed package objects; RepositorySet is a concrete struct with no injectable/overridable find_packages"]
fn test_most_stable_version_is_returned() {
    todo!()
}

#[test]
#[ignore = "requires mocking RepositorySet::find_packages with willReturnOnConsecutiveCalls; RepositorySet is a concrete struct with no injectable/overridable find_packages"]
fn test_most_stable_version_is_returned_regardless_of_order() {
    todo!()
}

#[test]
#[ignore = "requires mocking RepositorySet::find_packages to return fixed package objects; RepositorySet is a concrete struct with no injectable/overridable find_packages"]
fn test_highest_version_is_returned() {
    todo!()
}

#[test]
#[ignore = "requires mocking RepositorySet::find_packages to return fixed package objects; RepositorySet is a concrete struct with no injectable/overridable find_packages"]
fn test_highest_version_matching_stability_is_returned() {
    todo!()
}

#[test]
#[ignore = "requires mocking RepositorySet::find_packages to return fixed package objects; RepositorySet is a concrete struct with no injectable/overridable find_packages"]
fn test_most_stable_unstable_version_is_returned() {
    todo!()
}

#[test]
#[ignore = "requires mocking RepositorySet::find_packages to return fixed package objects; RepositorySet is a concrete struct with no injectable/overridable find_packages"]
fn test_default_branch_alias_is_never_returned() {
    todo!()
}

#[test]
#[ignore = "requires mocking RepositorySet::find_packages to return an empty list; RepositorySet is a concrete struct with no injectable/overridable find_packages"]
fn test_false_returned_on_no_packages() {
    todo!()
}

#[test]
#[ignore = "branch-alias cases need Package::set_extra, exposed only on RootPackageHandle, not on the PackageHandle/PackageInterfaceHandle passed to find_recommended_require_version"]
fn test_find_recommended_require_version() {
    todo!()
}
