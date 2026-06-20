//! ref: composer/tests/Composer/Test/Downloader/ZipDownloaderTest.php

// These construct a ZipDownloader with a mocked IO/HttpDownloader/ProcessExecutor and rely
// on ZipArchive extraction (todo!() in the php-shim) plus mocked unzip behaviour.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks IO/HttpDownloader/ProcessExecutor and uses ZipArchive (todo!())"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_error_messages);
stub!(test_zip_archive_only_failed);
stub!(test_zip_archive_extract_only_failed);
stub!(test_zip_archive_only_good);
stub!(test_system_unzip_only_failed);
stub!(test_system_unzip_only_good);
stub!(test_non_windows_fallback_good);
stub!(test_non_windows_fallback_failed);
