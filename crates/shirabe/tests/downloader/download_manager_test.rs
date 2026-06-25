//! ref: composer/tests/Composer/Test/Downloader/DownloadManagerTest.php

use std::cell::RefCell;
use std::rc::Rc;

use shirabe::downloader::DownloaderInterface;
use shirabe::downloader::download_manager::DownloadManager;
use shirabe::io::IOInterface;
use shirabe::package::handle::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe_semver::VersionParser;

use crate::downloader_stub::DownloaderStub;
use crate::io_stub::IOStub;

/// ref: DownloadManagerTest::createPackageMock
///
/// PHPUnit returns a `PackageInterface` mock; a real CompletePackage with the
/// relevant fields left at their defaults is an equivalent stand-in for the
/// installation-source/type dispatch logic exercised by the ported cases.
fn create_package_mock() -> PackageInterfaceHandle {
    let norm_version = VersionParser.normalize("1.0.0", None).unwrap();
    CompletePackageHandle::new("dummy/pkg".to_string(), norm_version, "1.0.0".to_string()).into()
}

/// ref: DownloadManagerTest::createDownloaderMock
fn create_downloader_mock() -> Rc<RefCell<dyn DownloaderInterface>> {
    Rc::new(RefCell::new(DownloaderStub::new())) as Rc<RefCell<dyn DownloaderInterface>>
}

fn create_manager() -> DownloadManager {
    let io = Rc::new(RefCell::new(IOStub::new())) as Rc<RefCell<dyn IOInterface>>;
    DownloadManager::new(io, false, None)
}

#[test]
fn test_set_get_downloader() {
    let downloader = create_downloader_mock();
    let mut manager = create_manager();

    manager.set_downloader("test", downloader.clone());
    assert!(Rc::ptr_eq(
        &downloader,
        &manager.get_downloader("test").unwrap()
    ));

    let result = manager.get_downloader("unregistered");
    assert!(result.is_err());
}

#[test]
fn test_get_downloader_for_incorrectly_installed_package() {
    // getInstallationSource() => null (the default for a fresh package).
    let package = create_package_mock();

    let manager = create_manager();

    let result = manager.get_downloader_for_package(package);
    assert!(result.is_err());
}

#[test]
fn test_get_downloader_for_metapackage() {
    let package = create_package_mock();
    package.__set_type("metapackage".to_string());

    let manager = create_manager();

    assert!(
        manager
            .get_downloader_for_package(package)
            .unwrap()
            .is_none()
    );
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloader"]
#[test]
fn test_get_downloader_for_correctly_installed_dist_package() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloader"]
#[test]
fn test_get_downloader_for_incorrectly_installed_dist_package() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloader"]
#[test]
fn test_get_downloader_for_correctly_installed_source_package() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloader"]
#[test]
fn test_get_downloader_for_incorrectly_installed_source_package() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_full_package_download() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_full_package_download_failover() {
    todo!()
}

#[ignore = "requires PHPUnit mock of PackageInterface (createPackageMock)"]
#[test]
fn test_bad_package_download() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_dist_only_package_download() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_source_only_package_download() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_metapackage_package_download() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_full_package_download_with_source_preferred() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_dist_only_package_download_with_source_preferred() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_source_only_package_download_with_source_preferred() {
    todo!()
}

#[ignore = "requires PHPUnit mock of PackageInterface (createPackageMock)"]
#[test]
fn test_bad_package_download_with_source_preferred() {
    todo!()
}

#[ignore = "requires PHPUnit mocks of PackageInterface and DownloaderInterface"]
#[test]
fn test_update_dist_with_equal_types() {
    todo!()
}

#[ignore = "requires PHPUnit mocks of PackageInterface and DownloaderInterface"]
#[test]
fn test_update_dist_with_not_equal_types() {
    todo!()
}

#[ignore = "requires PHPUnit mock of PackageInterface and ReflectionMethod for private getAvailableSources"]
#[test]
fn test_get_available_sources_update_sticks_to_same_source() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_update_metapackage() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_remove() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_metapackage_remove() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_install_preference_without_preference_dev() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_install_preference_without_preference_no_dev() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_install_preference_without_match_dev() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_install_preference_without_match_no_dev() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_install_preference_with_match_auto_dev() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_install_preference_with_match_auto_no_dev() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_install_preference_with_match_source() {
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_install_preference_with_match_dist() {
    todo!()
}
