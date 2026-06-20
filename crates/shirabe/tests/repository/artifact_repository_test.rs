//! ref: composer/tests/Composer/Test/Repository/ArtifactRepositoryTest.php

// ArtifactRepository::getPackages scans the fixture directory and opens each archive via
// ZipArchive / PharData, both of which are todo!() in the php-shim.

#[test]
#[ignore = "ArtifactRepository reads archives via ZipArchive/PharData, which are todo!() in the php-shim"]
fn test_extracts_configs_from_zip_archives() {
    todo!()
}

#[test]
#[ignore = "ArtifactRepository reads archives via ZipArchive/PharData, which are todo!() in the php-shim"]
fn test_absolute_repo_url_creates_absolute_url_packages() {
    todo!()
}

#[test]
#[ignore = "ArtifactRepository reads archives via ZipArchive/PharData, which are todo!() in the php-shim"]
fn test_relative_repo_url_creates_relative_url_packages() {
    todo!()
}
