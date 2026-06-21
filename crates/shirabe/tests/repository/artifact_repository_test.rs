//! ref: composer/tests/Composer/Test/Repository/ArtifactRepositoryTest.php

// ArtifactRepository::getPackages scans the fixture directory and opens each archive via
// ZipArchive / PharData, both of which are todo!() in the php-shim.

use shirabe_php_shim::extension_loaded;

fn set_up() {
    if !extension_loaded("zip") {
        // markTestSkipped('You need the zip extension to run this test.')
        todo!()
    }
}

#[test]
#[ignore = "ArtifactRepository exposes no public get_packages(); it does not impl RepositoryInterface and initialize/scan_directory are private"]
fn test_extracts_configs_from_zip_archives() {
    set_up();
    todo!()
}

#[test]
#[ignore = "ArtifactRepository exposes no public get_packages(); it does not impl RepositoryInterface and initialize/scan_directory are private"]
fn test_absolute_repo_url_creates_absolute_url_packages() {
    set_up();
    todo!()
}

#[test]
#[ignore = "ArtifactRepository exposes no public get_packages(); it does not impl RepositoryInterface and initialize/scan_directory are private"]
fn test_relative_repo_url_creates_relative_url_packages() {
    set_up();
    todo!()
}
