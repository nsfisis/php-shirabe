//! ref: composer/tests/Composer/Test/Util/Http/RequestProxyTest.php

use bstr::{BStr, BString};
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
    let options = curl_options(&[(CURLOPT_PROXY, PhpMixed::string(BString::default()))]);
    assert_eq!(options, proxy.get_curl_options(&IndexMap::new()).unwrap());
    assert!(proxy.get_context_options().is_none());
    assert_eq!("", proxy.get_status(None).unwrap());
}

#[test]
fn test_factory_no_proxy() {
    let proxy = RequestProxy::no_proxy();

    let options = curl_options(&[(CURLOPT_PROXY, PhpMixed::string(BString::default()))]);
    assert_eq!(options, proxy.get_curl_options(&IndexMap::new()).unwrap());
    assert!(proxy.get_context_options().is_none());
    assert_eq!("excluded by no_proxy", proxy.get_status(None).unwrap());
}

#[test]
fn test_is_secure() {
    let cases: Vec<(Option<&BStr>, bool)> = vec![
        (Some(BStr::new("http://proxy.com:80")), false),
        (Some(BStr::new("https://proxy.com:443")), true),
        (None, false),
    ];

    for (url, expected) in cases {
        let proxy = RequestProxy::new(url.map(BString::from), None, None, None);
        assert_eq!(expected, proxy.is_secure());
    }
}

#[test]
fn test_get_status_throws_on_bad_format_specifier() {
    let proxy = RequestProxy::new(
        Some("http://proxy.com:80".into()),
        None,
        None,
        Some("http://proxy.com:80".into()),
    );
    assert!(proxy.get_status(Some(BStr::new("using proxy"))).is_err());
}

#[test]
fn test_get_status() {
    let format = BStr::new("proxy (%s)");

    let cases: Vec<(Option<&BStr>, Option<&BStr>, &BStr)> = vec![
        (None, Some(format), BStr::new("")),
        (
            Some(BStr::new("http://proxy.com:80")),
            None,
            BStr::new("http://proxy.com:80"),
        ),
        (
            Some(BStr::new("http://proxy.com:80")),
            Some(format),
            BStr::new("proxy (http://proxy.com:80)"),
        ),
    ];

    for (url, format, expected) in cases {
        let format = format.map(bstr::BStr::new);
        let proxy = RequestProxy::new(url.map(BString::from), None, None, url.map(BString::from));

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
    let cases: Vec<(Option<&BStr>, Option<&BStr>, IndexMap<i64, PhpMixed>)> = vec![
        (
            None,
            None,
            curl_options(&[(CURLOPT_PROXY, PhpMixed::string(BString::default()))]),
        ),
        (
            Some(BStr::new("http://proxy.com:80")),
            None,
            curl_options(&[
                (
                    CURLOPT_PROXY,
                    PhpMixed::string(BString::from("http://proxy.com:80")),
                ),
                (CURLOPT_NOPROXY, PhpMixed::string(BString::default())),
            ]),
        ),
        (
            Some(BStr::new("http://proxy.com:80")),
            Some(BStr::new("user:p%40ss")),
            curl_options(&[
                (
                    CURLOPT_PROXY,
                    PhpMixed::string(BString::from("http://proxy.com:80")),
                ),
                (CURLOPT_NOPROXY, PhpMixed::string(BString::default())),
                (CURLOPT_PROXYAUTH, PhpMixed::Int(CURLAUTH_BASIC)),
                (
                    CURLOPT_PROXYUSERPWD,
                    PhpMixed::string(BString::from("user:p%40ss")),
                ),
            ]),
        ),
    ];

    for (url, auth, expected) in cases {
        let proxy = RequestProxy::new(url.map(BString::from), auth.map(BString::from), None, None);
        assert_eq!(expected, proxy.get_curl_options(&IndexMap::new()).unwrap());
    }
}

#[test]
fn test_get_curl_options_with_ssl() {
    let mut cafile_opts: IndexMap<BString, PhpMixed> = IndexMap::new();
    cafile_opts.insert(
        "cafile".into(),
        PhpMixed::string(BString::from("/certs/bundle.pem")),
    );

    let mut capath_opts: IndexMap<BString, PhpMixed> = IndexMap::new();
    capath_opts.insert("capath".into(), PhpMixed::string(BString::from("/certs")));

    let cases: Vec<(
        &BStr,
        Option<&BStr>,
        IndexMap<BString, PhpMixed>,
        IndexMap<i64, PhpMixed>,
    )> = vec![
        (
            BStr::new("https://proxy.com:443"),
            None,
            cafile_opts,
            curl_options(&[
                (
                    CURLOPT_PROXY,
                    PhpMixed::string(BString::from("https://proxy.com:443")),
                ),
                (CURLOPT_NOPROXY, PhpMixed::string(BString::default())),
                (
                    CURLOPT_PROXY_CAINFO,
                    PhpMixed::string(BString::from("/certs/bundle.pem")),
                ),
            ]),
        ),
        (
            BStr::new("https://proxy.com:443"),
            Some(BStr::new("user:p%40ss")),
            capath_opts,
            curl_options(&[
                (
                    CURLOPT_PROXY,
                    PhpMixed::string(BString::from("https://proxy.com:443")),
                ),
                (CURLOPT_NOPROXY, PhpMixed::string(BString::default())),
                (CURLOPT_PROXYAUTH, PhpMixed::Int(CURLAUTH_BASIC)),
                (
                    CURLOPT_PROXYUSERPWD,
                    PhpMixed::string(BString::from("user:p%40ss")),
                ),
                (
                    CURLOPT_PROXY_CAPATH,
                    PhpMixed::string(BString::from("/certs")),
                ),
            ]),
        ),
    ];

    for (url, auth, ssl_options, expected) in cases {
        let proxy = RequestProxy::new(Some(url.into()), auth.map(BString::from), None, None);
        assert_eq!(expected, proxy.get_curl_options(&ssl_options).unwrap());
    }
}

#[test]
fn test_get_status_keeps_non_utf8_bytes() {
    let url = BString::from(b"http://caf\xE9:80".as_slice());
    let proxy = RequestProxy::new(Some(url.clone()), None, None, Some(url));

    assert_eq!(
        b"proxy (http://caf\xE9:80)".as_slice(),
        proxy
            .get_status(Some(bstr::BStr::new("proxy (%s)")))
            .unwrap()
            .as_slice()
    );
}
