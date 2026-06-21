//! ref: composer/tests/Composer/Test/Repository/ComposerRepositoryTest.php

// These construct a ComposerRepository with a mocked HttpDownloader/IO/Config and parse
// provider/package data whose constraints go through a look-around regex; mocking is not
// available and a real HttpDownloader reaches curl_multi_init (todo!()).
#[ignore = "needs PHPUnit getMockBuilder to override loadRootServerFile; no method-mocking framework ported"]
#[test]
fn test_load_data() {
    todo!()
}

#[ignore = "needs getMockBuilder to override fetchFile plus ReflectionProperty/ReflectionMethod to set private props and invoke whatProvides; no mocking/reflection ported"]
#[test]
fn test_what_provides() {
    todo!()
}

#[ignore = "needs getHttpDownloaderMock test helper (HttpDownloaderMock not ported); real HttpDownloader hits todo!() curl I/O"]
#[test]
fn test_search_with_type() {
    todo!()
}

#[ignore = "needs getHttpDownloaderMock test helper (HttpDownloaderMock not ported); real HttpDownloader hits todo!() curl I/O"]
#[test]
fn test_search_with_special_chars() {
    todo!()
}

#[ignore = "needs getHttpDownloaderMock test helper (HttpDownloaderMock not ported); real HttpDownloader hits todo!() curl I/O"]
#[test]
fn test_search_with_abandoned_packages() {
    todo!()
}

#[ignore = "needs getMockBuilder HttpDownloader mock plus ReflectionObject getMethod/getProperty to set private url and invoke canonicalizeUrl; no mocking/reflection ported"]
#[test]
fn test_canonicalize_url() {
    todo!()
}

#[ignore = "needs getHttpDownloaderMock test helper (HttpDownloaderMock not ported); real HttpDownloader hits todo!() curl I/O"]
#[test]
fn test_get_provider_names_will_return_partial_package_names() {
    todo!()
}

#[ignore = "needs getHttpDownloaderMock test helper (HttpDownloaderMock not ported); real HttpDownloader hits todo!() curl I/O"]
#[test]
fn test_get_security_advisories_assert_repository_http_options_are_used() {
    todo!()
}

#[ignore = "needs getHttpDownloaderMock test helper (HttpDownloaderMock not ported); real HttpDownloader hits todo!() curl I/O"]
#[test]
fn test_get_security_advisories_assert_repository_advisories_is_zero_indexed_array_with_consecutive_keys()
 {
    todo!()
}
