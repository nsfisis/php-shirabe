//! ref: composer/tests/Composer/Test/Downloader/FossilDownloaderTest.php

// Every case constructs a FossilDownloader with a mocked IO/Config and a mocked
// ProcessExecutor to feed fossil command output; a real HttpDownloader reaches
// curl_multi_init (todo!()), and ProcessExecutor mocking is not available.

#[test]
#[ignore = "mocks ProcessExecutor/IO and needs an HttpDownloader (curl_multi_init todo!())"]
fn test_install_for_package_without_source_reference() {
    todo!()
}

#[test]
#[ignore = "mocks ProcessExecutor/IO and needs an HttpDownloader (curl_multi_init todo!())"]
fn test_install() {
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
