//! ref: composer/tests/Composer/Test/Downloader/FileDownloaderTest.php

fn set_up() {
    // The HttpDownloader mock (disableOriginalConstructor) is not ported.
    todo!()
}

// These construct a FileDownloader with a mocked IO/HttpDownloader (curl_multi_init todo!())
// and a mocked Cache/Package to drive download/checksum behaviour.
#[test]
#[ignore = "requires PHPUnit mock of HttpDownloader::addCopy (getDownloader helper); no mocking framework and real HttpDownloader reaches curl_multi_init todo!()"]
fn test_download_for_package_without_dist_reference() {
    set_up();
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mock of HttpDownloader::addCopy (getDownloader helper); no mocking framework and real HttpDownloader reaches curl_multi_init todo!()"]
fn test_download_to_existing_file() {
    set_up();
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mock of HttpDownloader::addCopy (getDownloader helper) and ReflectionMethod on private getFileName; no mocking/reflection framework"]
fn test_get_file_name() {
    set_up();
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mock of HttpDownloader::addCopy and IOInterface::write callback; no mocking framework and real HttpDownloader reaches curl_multi_init todo!()"]
fn test_download_but_file_is_unsaved() {
    set_up();
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mocks of Cache::copyTo/copyFrom and HttpDownloader::addCopy with assertion callbacks; no mocking framework"]
fn test_download_with_custom_processed_url() {
    set_up();
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mocks of Cache::copyTo/copyFrom and HttpDownloader::addCopy with assertion callbacks; no mocking framework"]
fn test_download_with_custom_cache_key() {
    set_up();
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mock of Cache::gcIsNecessary/gc with expectation tracking; no mocking framework"]
fn test_cache_garbage_collection_is_called() {
    set_up();
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mock of Filesystem and HttpDownloader::addCopy plus ReflectionMethod on private getFileName; no mocking/reflection framework"]
fn test_download_file_with_invalid_checksum() {
    set_up();
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mocks of Filesystem::removeDirectoryAsync/normalizePath, HttpDownloader::addCopy, getIOMock expectations and ReflectionMethod on private getFileName; no mocking/reflection framework"]
fn test_downgrade_shows_appropriate_message() {
    set_up();
    todo!()
}
