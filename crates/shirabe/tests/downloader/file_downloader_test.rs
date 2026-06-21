//! ref: composer/tests/Composer/Test/Downloader/FileDownloaderTest.php

fn set_up() {
    // The HttpDownloader mock (disableOriginalConstructor) is not ported.
    todo!()
}

// These construct a FileDownloader with a mocked IO/HttpDownloader (curl_multi_init todo!())
// and a mocked Cache/Package to drive download/checksum behaviour.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks IO/HttpDownloader (curl_multi_init todo!()) and Cache/Package"]
        fn $name() {
            set_up();
            todo!()
        }
    };
}

stub!(test_download_for_package_without_dist_reference);
stub!(test_download_to_existing_file);
stub!(test_get_file_name);
stub!(test_download_but_file_is_unsaved);
stub!(test_download_with_custom_processed_url);
stub!(test_download_with_custom_cache_key);
stub!(test_cache_garbage_collection_is_called);
stub!(test_download_file_with_invalid_checksum);
stub!(test_downgrade_shows_appropriate_message);
