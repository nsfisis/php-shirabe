//! ref: composer/tests/Composer/Test/Package/Archiver/ArchiveManagerTest.php

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::downloader::DownloadManager;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::CompletePackageInterfaceHandle;
use shirabe::package::archiver::{ArchiveManager, PharArchiver, ZipArchiver};
use shirabe::package::handle::CompletePackageHandle;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe::util::r#loop::Loop;
use shirabe_php_shim::realpath;
use std::cell::RefCell;
use std::rc::Rc;
use tempfile::TempDir;

// ref: ArchiverTestCase::setUp + ArchiveManagerTest::setUp.
//
// The PHP test builds the ArchiveManager via Factory/FactoryMock; here we construct it directly
// with the same archivers (ZipArchiver + PharArchiver) that Factory::createArchiveManager adds.
struct TestCase {
    manager: ArchiveManager,
    test_dir: String,
    target_dir: String,
    _test_dir_guard: TempDir,
}

impl TestCase {
    fn set_up() -> Self {
        let guard = TempDir::new().unwrap();
        let test_dir = guard.path().to_string_lossy().to_string();

        let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
        let config = Rc::new(RefCell::new(Config::new(false, None)));
        // The filename/unknown-format tests never drive the download path, so a mock
        // HttpDownloader (no curl backend) is sufficient to satisfy Loop's dependency.
        let http_downloader = Rc::new(RefCell::new(HttpDownloader::__new_mock(io.clone(), config)));
        let dm = Rc::new(RefCell::new(DownloadManager::new(io.clone(), false, None)));
        let r#loop = Rc::new(RefCell::new(Loop::new(http_downloader, None)));

        let mut manager = ArchiveManager::new(dm, r#loop);
        manager.add_archiver(Box::new(ZipArchiver::new()));
        manager.add_archiver(Box::new(PharArchiver::new()));

        let target_dir = format!("{}/composer_archiver_tests", test_dir);

        Self {
            manager,
            test_dir,
            target_dir,
            _test_dir_guard: guard,
        }
    }

    // ref: ArchiverTestCase::setupPackage.
    fn setup_package(&self) -> CompletePackageInterfaceHandle {
        let package = CompletePackageHandle::new(
            "archivertest/archivertest".to_string(),
            "master".to_string(),
            "master".to_string(),
        );
        package.set_source_url(Some(realpath(&self.test_dir).unwrap_or_default()));
        package.set_source_reference(Some("master".to_string()));
        package.__set_source_type(Some("git".to_string()));

        package.into()
    }
}

#[test]
fn test_unknown_format() {
    let mut test_case = TestCase::set_up();
    let package = test_case.setup_package();

    let result = test_case.manager.archive(
        package,
        "__unknown_format__".to_string(),
        test_case.target_dir.clone(),
        None,
        false,
    );

    let err = result.expect_err("expected RuntimeException for unknown format");
    assert!(
        err.downcast_ref::<shirabe_php_shim::RuntimeException>()
            .is_some()
    );
}

// ref: ArchiveManagerTest::testArchiveTar / testArchiveCustomFileName.
//
// These drive ArchiveManager::archive end-to-end for the 'tar' format, which dispatches to
// PharArchiver::archive. That builds the archive via PharData, whose build_from_iterator is
// todo!() in the php-shim, so the archiving path cannot run yet.
#[test]
#[ignore = "needs PharData tar archiving (build_from_iterator is todo!() in the php-shim) for ArchiveManager::archive('tar', ...)"]
fn test_archive_tar() {
    todo!()
}

#[test]
#[ignore = "needs PharData tar archiving (build_from_iterator is todo!() in the php-shim) for ArchiveManager::archive('tar', ...)"]
fn test_archive_custom_file_name() {
    todo!()
}

#[test]
fn test_get_package_filename_parts() {
    let test_case = TestCase::set_up();
    let package = test_case.setup_package();

    let mut expected: IndexMap<String, String> = IndexMap::new();
    expected.insert("base".to_string(), "archivertest-archivertest".to_string());
    expected.insert("version".to_string(), "master".to_string());
    expected.insert("source_reference".to_string(), "4f26ae".to_string());

    assert_eq!(
        expected,
        test_case
            .manager
            .get_package_filename_parts(package)
            .unwrap()
    );
}

#[test]
fn test_get_package_filename() {
    let test_case = TestCase::set_up();
    let package = test_case.setup_package();

    assert_eq!(
        "archivertest-archivertest-master-4f26ae",
        test_case.manager.get_package_filename(package).unwrap()
    );
}
