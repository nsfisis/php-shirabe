//! ref: composer/tests/Composer/Test/Downloader/PerforceDownloaderTest.php

use tempfile::TempDir;

fn set_up() -> TempDir {
    let test_path = TempDir::new().unwrap();
    // repoConfig/config/io/processExecutor/repository/package/downloader rely on
    // ProcessExecutorMock and PHPUnit mocks of the repository and Package, which are not
    // ported.
    let () = todo!();
    #[allow(unreachable_code)]
    test_path
}

// These mock Perforce, the repository config and a Package to drive PerforceDownloader's
// initialization and install paths; mocking is not available here.

#[test]
#[ignore = "mocks Perforce/repository/Package; mocking is not available"]
fn test_init_perforce_instantiates_a_new_perforce_object() {
    let _test_path = set_up();
    todo!()
}

#[test]
#[ignore = "mocks Perforce/repository/Package; mocking is not available"]
fn test_init_perforce_does_nothing_if_perforce_already_set() {
    let _test_path = set_up();
    todo!()
}

#[test]
#[ignore = "mocks Perforce/repository/Package; mocking is not available"]
fn test_do_install_with_tag() {
    let _test_path = set_up();
    todo!()
}

#[test]
#[ignore = "mocks Perforce/repository/Package; mocking is not available"]
fn test_do_install_with_no_tag() {
    let _test_path = set_up();
    todo!()
}
