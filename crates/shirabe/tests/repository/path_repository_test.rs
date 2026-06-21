//! ref: composer/tests/Composer/Test/Repository/PathRepositoryTest.php

// PathRepository does not implement RepositoryInterface and exposes no public
// getPackages/count/hasPackage; the delegation to its inner ArrayRepository is not yet
// ported, so there is no way to drive these tests. The unversioned cases additionally
// require the VersionGuesser (git).

#[ignore = "PathRepository exposes no public RepositoryInterface::get_packages/count/has_package; the inner ArrayRepository delegation is not ported"]
#[test]
fn test_load_package_from_file_system_with_incorrect_path() {
    todo!()
}

#[ignore = "PathRepository exposes no public RepositoryInterface::get_packages/count/has_package; the inner ArrayRepository delegation is not ported"]
#[test]
fn test_load_package_from_file_system_with_version() {
    todo!()
}

#[ignore = "PathRepository exposes no public RepositoryInterface::get_packages/count/has_package; the inner ArrayRepository delegation is not ported"]
#[test]
fn test_load_package_from_file_system_without_version() {
    todo!()
}

#[ignore = "PathRepository exposes no public RepositoryInterface::get_packages/count/has_package; the inner ArrayRepository delegation is not ported"]
#[test]
fn test_load_package_from_file_system_with_wildcard() {
    todo!()
}

#[ignore = "PathRepository exposes no public RepositoryInterface::get_packages/count/has_package; the inner ArrayRepository delegation is not ported"]
#[test]
fn test_load_package_with_explicit_versions() {
    todo!()
}

#[ignore = "PathRepository exposes no public RepositoryInterface::get_packages/count/has_package; the inner ArrayRepository delegation is not ported"]
#[test]
fn test_url_remains_relative() {
    todo!()
}

#[ignore = "PathRepository exposes no public RepositoryInterface::get_packages/count/has_package; the inner ArrayRepository delegation is not ported"]
#[test]
fn test_reference_none() {
    todo!()
}

#[ignore = "PathRepository exposes no public RepositoryInterface::get_packages/count/has_package; the inner ArrayRepository delegation is not ported"]
#[test]
fn test_reference_config() {
    todo!()
}
