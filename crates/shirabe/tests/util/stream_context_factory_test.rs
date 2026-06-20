//! ref: composer/tests/Composer/Test/Util/StreamContextFactoryTest.php

// These build a stream context and assert proxy/option handling driven by HTTP(S)_PROXY /
// no_proxy environment variables; the env-dependent setup (without its setUp/tearDown
// isolation) is not ported.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "not yet ported (StreamContextFactory proxy/option building is driven by proxy env vars)"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_get_context);
stub!(test_http_proxy);
stub!(test_http_proxy_with_no_proxy);
stub!(test_http_proxy_with_no_proxy_wildcard);
stub!(test_options_are_preserved);
stub!(test_http_proxy_without_port);
stub!(test_https_proxy_override);
stub!(test_ssl_proxy);
stub!(test_ensure_thatfix_http_header_field_moves_content_type_to_end_of_options);
stub!(test_init_options_does_include_proxy_auth_headers);
stub!(test_init_options_for_curl_does_not_include_proxy_auth_headers);
