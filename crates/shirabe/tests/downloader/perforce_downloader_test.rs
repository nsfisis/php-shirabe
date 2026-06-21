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

#[ignore = "requires PHPUnit getMockBuilder mocks of IOInterface/PackageInterface/VcsRepository and ProcessExecutorMock via unported set_up()"]
#[test]
fn test_init_perforce_instantiates_a_new_perforce_object() {
    let _test_path = set_up();
    todo!()
}

#[ignore = "requires PHPUnit getMockBuilder mock of Composer\\Util\\Perforce and expects()->never() verification, unavailable in the Rust port"]
#[test]
fn test_init_perforce_does_nothing_if_perforce_already_set() {
    let _test_path = set_up();
    todo!()
}

#[ignore = "requires PHPUnit getMockBuilder mock of Composer\\Util\\Perforce with expects()->once()->method() expectation verification, unavailable in the Rust port"]
#[test]
fn test_do_install_with_tag() {
    let _test_path = set_up();
    todo!()
}

#[ignore = "requires PHPUnit getMockBuilder mock of Composer\\Util\\Perforce with expects()->once()->method() expectation verification, unavailable in the Rust port"]
#[test]
fn test_do_install_with_no_tag() {
    let _test_path = set_up();
    todo!()
}
