//! ref: composer/tests/Composer/Test/Downloader/DownloadManagerTest.php

fn set_up() {
    // The Filesystem and IO mocks are not ported.
    todo!()
}

// These mock IO and individual downloaders to drive DownloadManager's selection/download/
// update/remove logic; mocking is not available here.
#[ignore = "requires PHPUnit mock of DownloaderInterface (createDownloaderMock)"]
#[test]
fn test_set_get_downloader() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mock of PackageInterface (createPackageMock)"]
#[test]
fn test_get_downloader_for_incorrectly_installed_package() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloader"]
#[test]
fn test_get_downloader_for_correctly_installed_dist_package() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloader"]
#[test]
fn test_get_downloader_for_incorrectly_installed_dist_package() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloader"]
#[test]
fn test_get_downloader_for_correctly_installed_source_package() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloader"]
#[test]
fn test_get_downloader_for_incorrectly_installed_source_package() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mock of PackageInterface (createPackageMock)"]
#[test]
fn test_get_downloader_for_metapackage() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_full_package_download() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_full_package_download_failover() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mock of PackageInterface (createPackageMock)"]
#[test]
fn test_bad_package_download() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_dist_only_package_download() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_source_only_package_download() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_metapackage_package_download() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_full_package_download_with_source_preferred() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_dist_only_package_download_with_source_preferred() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_source_only_package_download_with_source_preferred() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mock of PackageInterface (createPackageMock)"]
#[test]
fn test_bad_package_download_with_source_preferred() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks of PackageInterface and DownloaderInterface"]
#[test]
fn test_update_dist_with_equal_types() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks of PackageInterface and DownloaderInterface"]
#[test]
fn test_update_dist_with_not_equal_types() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mock of PackageInterface and ReflectionMethod for private getAvailableSources"]
#[test]
fn test_get_available_sources_update_sticks_to_same_source() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_update_metapackage() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_remove() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_metapackage_remove() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_install_preference_without_preference_dev() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_install_preference_without_preference_no_dev() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_install_preference_without_match_dev() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_install_preference_without_match_no_dev() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_install_preference_with_match_auto_dev() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_install_preference_with_match_auto_no_dev() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_install_preference_with_match_source() {
    set_up();
    todo!()
}

#[ignore = "requires PHPUnit mocks and partial mock of DownloadManager::getDownloaderForPackage"]
#[test]
fn test_install_preference_with_match_dist() {
    set_up();
    todo!()
}
