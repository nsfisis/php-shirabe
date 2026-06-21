//! ref: composer/tests/Composer/Test/Util/RemoteFilesystemTest.php

// These mock IO/Config/HttpDownloader and use reflection to drive RemoteFilesystem option
// building and downloads; mocking/reflection are not available and a real HttpDownloader
// reaches curl_multi_init (todo!()).
#[test]
#[ignore = "requires a mocked IOInterface (expects(once())->hasAuthentication) and ReflectionMethod to invoke private get_options_for_url plus ReflectionProperty on private file_url; no mocking/reflection infrastructure exists"]
fn test_get_options_for_url() {
    todo!()
}

#[test]
#[ignore = "requires a mocked IOInterface (expects(once())->hasAuthentication/getAuthentication) and ReflectionMethod to invoke private get_options_for_url plus ReflectionProperty on private file_url; no mocking/reflection infrastructure exists"]
fn test_get_options_for_url_with_authorization() {
    todo!()
}

#[test]
#[ignore = "requires a mocked IOInterface (expects(once())->hasAuthentication/getAuthentication) and ReflectionMethod to invoke private get_options_for_url plus ReflectionProperty on private file_url; no mocking/reflection infrastructure exists"]
fn test_get_options_for_url_with_stream_options() {
    todo!()
}

#[test]
#[ignore = "requires a mocked IOInterface (expects(once())->hasAuthentication/getAuthentication) and ReflectionMethod to invoke private get_options_for_url plus ReflectionProperty on private file_url; no mocking/reflection infrastructure exists"]
fn test_get_options_for_url_with_call_options_keeps_header() {
    todo!()
}

#[test]
#[ignore = "requires a mocked IOInterface and ReflectionMethod to invoke private callback_get plus ReflectionProperty to read private bytes_max; no mocking/reflection infrastructure exists"]
fn test_callback_get_file_size() {
    todo!()
}

#[test]
#[ignore = "requires a mocked IOInterface (expects(once())->overwriteError) and ReflectionProperty to set private bytes_max/progress and read private last_progress; no mocking/reflection infrastructure exists"]
fn test_callback_get_notify_progress() {
    todo!()
}

#[test]
#[ignore = "requires a mocked IOInterface and ReflectionMethod to invoke private callback_get; no mocking/reflection infrastructure exists"]
fn test_callback_get_passes_through404() {
    todo!()
}

#[test]
#[ignore = "requires a mocked IOInterface and a real get_contents which bottoms at curl_multi_init (todo!()); no mocking infrastructure exists"]
fn test_get_contents() {
    todo!()
}

#[test]
#[ignore = "requires a mocked IOInterface and a real copy which bottoms at curl_multi_init (todo!()); no mocking infrastructure exists"]
fn test_copy() {
    todo!()
}

#[test]
#[ignore = "requires a MockObject subclass of RemoteFilesystem overriding private get_remote_contents; no mocking infrastructure exists"]
fn test_copy_with_no_retry_on_failure() {
    todo!()
}

#[test]
#[ignore = "requires MockObject subclasses overriding RemoteFilesystem::get_remote_contents and AuthHelper::prompt_auth_if_needed; no mocking infrastructure exists"]
fn test_copy_with_success_on_retry() {
    todo!()
}

#[test]
#[ignore = "requires a mocked IOInterface and ReflectionMethod to invoke private get_options_for_url plus ReflectionProperty on private file_url; no mocking/reflection infrastructure exists"]
fn test_get_options_for_url_creates_secure_tls_defaults() {
    todo!()
}

#[test]
#[ignore = "requires a mocked ConsoleIO (getMockBuilder disableOriginalConstructor) and a real getContents network download reaching curl_multi_init (todo!()); no mocking infrastructure exists"]
fn test_bit_bucket_public_download() {
    todo!()
}
