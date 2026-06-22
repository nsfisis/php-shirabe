//! ref: composer/tests/Composer/Test/Util/Http/RequestProxyTest.php

use indexmap::IndexMap;
use shirabe::util::http::request_proxy::RequestProxy;
use shirabe_php_shim::{
    CURLAUTH_BASIC, CURLOPT_NOPROXY, CURLOPT_PROXY, CURLOPT_PROXY_CAINFO, CURLOPT_PROXY_CAPATH,
    CURLOPT_PROXYAUTH, CURLOPT_PROXYUSERPWD, PhpMixed,
};

fn curl_options(pairs: &[(i64, PhpMixed)]) -> IndexMap<i64, PhpMixed> {
    pairs.iter().cloned().collect()
}

#[test]
fn test_factory_none() {
    let proxy = RequestProxy::none();

    // extension_loaded('curl') is always true in the php-shim.
    let options = curl_options(&[(CURLOPT_PROXY, PhpMixed::String(String::new()))]);
    assert_eq!(options, proxy.get_curl_options(&IndexMap::new()).unwrap());
    assert!(proxy.get_context_options().is_none());
    assert_eq!("", proxy.get_status(None).unwrap());
}

#[test]
fn test_factory_no_proxy() {
    let proxy = RequestProxy::no_proxy();

    let options = curl_options(&[(CURLOPT_PROXY, PhpMixed::String(String::new()))]);
    assert_eq!(options, proxy.get_curl_options(&IndexMap::new()).unwrap());
    assert!(proxy.get_context_options().is_none());
    assert_eq!("excluded by no_proxy", proxy.get_status(None).unwrap());
}

#[test]
fn test_is_secure() {
    let cases: Vec<(Option<&str>, bool)> = vec![
        (Some("http://proxy.com:80"), false),
        (Some("https://proxy.com:443"), true),
        (None, false),
    ];

    for (url, expected) in cases {
        let proxy = RequestProxy::new(url.map(String::from), None, None, None);
        assert_eq!(expected, proxy.is_secure());
    }
}

#[test]
fn test_get_status_throws_on_bad_format_specifier() {
    let proxy = RequestProxy::new(
        Some("http://proxy.com:80".to_string()),
        None,
        None,
        Some("http://proxy.com:80".to_string()),
    );
    assert!(proxy.get_status(Some("using proxy")).is_err());
}

#[test]
fn test_get_status() {
    let format = "proxy (%s)";

    let cases: Vec<(Option<&str>, Option<&str>, &str)> = vec![
        (None, Some(format), ""),
        (Some("http://proxy.com:80"), None, "http://proxy.com:80"),
        (
            Some("http://proxy.com:80"),
            Some(format),
            "proxy (http://proxy.com:80)",
        ),
    ];

    for (url, format, expected) in cases {
        let proxy = RequestProxy::new(url.map(String::from), None, None, url.map(String::from));

        if format.is_none() {
            // try with and without optional param
            assert_eq!(expected, proxy.get_status(None).unwrap());
            assert_eq!(expected, proxy.get_status(format).unwrap());
        } else {
            assert_eq!(expected, proxy.get_status(format).unwrap());
        }
    }
}

#[test]
fn test_get_curl_options() {
    let cases: Vec<(Option<&str>, Option<&str>, IndexMap<i64, PhpMixed>)> = vec![
        (
            None,
            None,
            curl_options(&[(CURLOPT_PROXY, PhpMixed::String(String::new()))]),
        ),
        (
            Some("http://proxy.com:80"),
            None,
            curl_options(&[
                (
                    CURLOPT_PROXY,
                    PhpMixed::String("http://proxy.com:80".to_string()),
                ),
                (CURLOPT_NOPROXY, PhpMixed::String(String::new())),
            ]),
        ),
        (
            Some("http://proxy.com:80"),
            Some("user:p%40ss"),
            curl_options(&[
                (
                    CURLOPT_PROXY,
                    PhpMixed::String("http://proxy.com:80".to_string()),
                ),
                (CURLOPT_NOPROXY, PhpMixed::String(String::new())),
                (CURLOPT_PROXYAUTH, PhpMixed::Int(CURLAUTH_BASIC)),
                (
                    CURLOPT_PROXYUSERPWD,
                    PhpMixed::String("user:p%40ss".to_string()),
                ),
            ]),
        ),
    ];

    for (url, auth, expected) in cases {
        let proxy = RequestProxy::new(url.map(String::from), auth.map(String::from), None, None);
        assert_eq!(expected, proxy.get_curl_options(&IndexMap::new()).unwrap());
    }
}

#[test]
#[ignore]
fn test_get_curl_options_with_ssl() {
    let mut cafile_opts: IndexMap<String, PhpMixed> = IndexMap::new();
    cafile_opts.insert(
        "cafile".to_string(),
        PhpMixed::String("/certs/bundle.pem".to_string()),
    );

    let mut capath_opts: IndexMap<String, PhpMixed> = IndexMap::new();
    capath_opts.insert("capath".to_string(), PhpMixed::String("/certs".to_string()));

    let cases: Vec<(
        &str,
        Option<&str>,
        IndexMap<String, PhpMixed>,
        IndexMap<i64, PhpMixed>,
    )> = vec![
        (
            "https://proxy.com:443",
            None,
            cafile_opts,
            curl_options(&[
                (
                    CURLOPT_PROXY,
                    PhpMixed::String("https://proxy.com:443".to_string()),
                ),
                (CURLOPT_NOPROXY, PhpMixed::String(String::new())),
                (
                    CURLOPT_PROXY_CAINFO,
                    PhpMixed::String("/certs/bundle.pem".to_string()),
                ),
            ]),
        ),
        (
            "https://proxy.com:443",
            Some("user:p%40ss"),
            capath_opts,
            curl_options(&[
                (
                    CURLOPT_PROXY,
                    PhpMixed::String("https://proxy.com:443".to_string()),
                ),
                (CURLOPT_NOPROXY, PhpMixed::String(String::new())),
                (CURLOPT_PROXYAUTH, PhpMixed::Int(CURLAUTH_BASIC)),
                (
                    CURLOPT_PROXYUSERPWD,
                    PhpMixed::String("user:p%40ss".to_string()),
                ),
                (CURLOPT_PROXY_CAPATH, PhpMixed::String("/certs".to_string())),
            ]),
        ),
    ];

    for (url, auth, ssl_options, expected) in cases {
        let proxy = RequestProxy::new(Some(url.to_string()), auth.map(String::from), None, None);
        assert_eq!(expected, proxy.get_curl_options(&ssl_options).unwrap());
    }
}
