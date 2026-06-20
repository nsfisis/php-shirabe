//! ref: composer/tests/Composer/Test/Repository/ComposerRepositoryTest.php

// These construct a ComposerRepository with a mocked HttpDownloader/IO/Config and parse
// provider/package data whose constraints go through a look-around regex; mocking is not
// available and a real HttpDownloader reaches curl_multi_init (todo!()).
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks HttpDownloader/IO (curl_multi_init todo!()) and parses constraints via a look-around regex"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_load_data);
stub!(test_what_provides);
stub!(test_search_with_type);
stub!(test_search_with_special_chars);
stub!(test_search_with_abandoned_packages);
stub!(test_canonicalize_url);
stub!(test_get_provider_names_will_return_partial_package_names);
stub!(test_get_security_advisories_assert_repository_http_options_are_used);
stub!(test_get_security_advisories_assert_repository_advisories_is_zero_indexed_array_with_consecutive_keys);
