//! ref: composer/tests/Composer/Test/Downloader/HgDownloaderTest.php

// Every case constructs an HgDownloader with a mocked IO/Config and a mocked
// ProcessExecutor to feed hg command output; a real HttpDownloader reaches
// curl_multi_init (todo!()), and ProcessExecutor mocking is not available.

#[test]
#[ignore = "mocks ProcessExecutor/IO and needs an HttpDownloader (curl_multi_init todo!())"]
fn test_download_for_package_without_source_reference() {
    todo!()
}

#[test]
#[ignore = "mocks ProcessExecutor/IO and needs an HttpDownloader (curl_multi_init todo!())"]
fn test_download() {
    todo!()
}

#[test]
#[ignore = "mocks ProcessExecutor/IO and needs an HttpDownloader (curl_multi_init todo!())"]
fn test_updatefor_package_without_source_reference() {
    todo!()
}

#[test]
#[ignore = "mocks ProcessExecutor/IO and needs an HttpDownloader (curl_multi_init todo!())"]
fn test_update() {
    todo!()
}

#[test]
#[ignore = "mocks ProcessExecutor/IO and needs an HttpDownloader (curl_multi_init todo!())"]
fn test_remove() {
    todo!()
}

#[test]
#[ignore = "mocks ProcessExecutor/IO and needs an HttpDownloader (curl_multi_init todo!())"]
fn test_get_installation_source() {
    todo!()
}
