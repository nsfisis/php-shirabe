//! ref: composer/tests/Composer/Test/Util/RemoteFilesystemTest.php

// These mock IO/Config/HttpDownloader and use reflection to drive RemoteFilesystem option
// building and downloads; mocking/reflection are not available and a real HttpDownloader
// reaches curl_multi_init (todo!()).
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks IO/Config/HttpDownloader (curl_multi_init todo!()) and uses reflection; not ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_get_options_for_url);
stub!(test_get_options_for_url_with_authorization);
stub!(test_get_options_for_url_with_stream_options);
stub!(test_get_options_for_url_with_call_options_keeps_header);
stub!(test_callback_get_file_size);
stub!(test_callback_get_notify_progress);
stub!(test_callback_get_passes_through404);
stub!(test_get_contents);
stub!(test_copy);
stub!(test_copy_with_no_retry_on_failure);
stub!(test_copy_with_success_on_retry);
stub!(test_get_options_for_url_creates_secure_tls_defaults);
stub!(test_bit_bucket_public_download);
