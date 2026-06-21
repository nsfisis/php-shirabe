//! ref: composer/tests/Composer/Test/Util/RemoteFilesystemTest.php

// These mock IO/Config/HttpDownloader and use reflection to drive RemoteFilesystem option
// building and downloads; mocking/reflection are not available and a real HttpDownloader
// reaches curl_multi_init (todo!()).
#[test]
#[ignore = "mocks IO/Config/HttpDownloader (curl_multi_init todo!()) and uses reflection; not ported"]
fn test_get_options_for_url() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/HttpDownloader (curl_multi_init todo!()) and uses reflection; not ported"]
fn test_get_options_for_url_with_authorization() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/HttpDownloader (curl_multi_init todo!()) and uses reflection; not ported"]
fn test_get_options_for_url_with_stream_options() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/HttpDownloader (curl_multi_init todo!()) and uses reflection; not ported"]
fn test_get_options_for_url_with_call_options_keeps_header() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/HttpDownloader (curl_multi_init todo!()) and uses reflection; not ported"]
fn test_callback_get_file_size() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/HttpDownloader (curl_multi_init todo!()) and uses reflection; not ported"]
fn test_callback_get_notify_progress() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/HttpDownloader (curl_multi_init todo!()) and uses reflection; not ported"]
fn test_callback_get_passes_through404() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/HttpDownloader (curl_multi_init todo!()) and uses reflection; not ported"]
fn test_get_contents() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/HttpDownloader (curl_multi_init todo!()) and uses reflection; not ported"]
fn test_copy() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/HttpDownloader (curl_multi_init todo!()) and uses reflection; not ported"]
fn test_copy_with_no_retry_on_failure() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/HttpDownloader (curl_multi_init todo!()) and uses reflection; not ported"]
fn test_copy_with_success_on_retry() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/HttpDownloader (curl_multi_init todo!()) and uses reflection; not ported"]
fn test_get_options_for_url_creates_secure_tls_defaults() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/HttpDownloader (curl_multi_init todo!()) and uses reflection; not ported"]
fn test_bit_bucket_public_download() {
    todo!()
}
