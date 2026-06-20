//! ref: composer/tests/Composer/Test/Repository/PathRepositoryTest.php

// PathRepository does not implement RepositoryInterface and exposes no public
// getPackages/count/hasPackage; the delegation to its inner ArrayRepository is not yet
// ported, so there is no way to drive these tests. The unversioned cases additionally
// require the VersionGuesser (git).

#[test]
#[ignore = "PathRepository exposes no public getPackages/count/hasPackage (RepositoryInterface delegation not ported)"]
fn test_load_package_from_file_system_with_incorrect_path() {
    todo!()
}

#[test]
#[ignore = "PathRepository exposes no public getPackages/count/hasPackage (RepositoryInterface delegation not ported)"]
fn test_load_package_from_file_system_with_version() {
    todo!()
}

#[test]
#[ignore = "PathRepository exposes no public getPackages/count/hasPackage (RepositoryInterface delegation not ported)"]
fn test_load_package_from_file_system_without_version() {
    todo!()
}

#[test]
#[ignore = "PathRepository exposes no public getPackages/count/hasPackage (RepositoryInterface delegation not ported)"]
fn test_load_package_from_file_system_with_wildcard() {
    todo!()
}

#[test]
#[ignore = "PathRepository exposes no public getPackages/count/hasPackage (RepositoryInterface delegation not ported)"]
fn test_load_package_with_explicit_versions() {
    todo!()
}

#[test]
#[ignore = "PathRepository exposes no public getPackages/count/hasPackage (RepositoryInterface delegation not ported)"]
fn test_url_remains_relative() {
    todo!()
}

#[test]
#[ignore = "PathRepository exposes no public getPackages/count/hasPackage (RepositoryInterface delegation not ported)"]
fn test_reference_none() {
    todo!()
}

#[test]
#[ignore = "PathRepository exposes no public getPackages/count/hasPackage (RepositoryInterface delegation not ported)"]
fn test_reference_config() {
    todo!()
}
