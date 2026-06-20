//! ref: composer/tests/Composer/Test/Util/Http/ProxyManagerTest.php

// ProxyManager reads HTTP(S)_PROXY / CGI_HTTP_PROXY / no_proxy environment variables; the
// env-dependent setup (without its setUp/tearDown isolation) is not ported.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "not yet ported (ProxyManager is driven by proxy environment variables)"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_instantiation);
stub!(test_get_proxy_for_request_throws_on_bad_proxy_url);
stub!(test_lowercase_overrides_uppercase);
stub!(test_cgi_proxy_is_only_used_when_no_http_proxy);
stub!(test_no_http_proxy_does_not_use_https_proxy);
stub!(test_no_https_proxy_does_not_use_http_proxy);
stub!(test_get_proxy_for_request);
