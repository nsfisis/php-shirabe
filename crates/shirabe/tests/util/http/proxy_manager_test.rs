//! ref: composer/tests/Composer/Test/Util/Http/ProxyManagerTest.php sha1:46e551dbac42c7a0306d66a89c7a05863a80c932

// ProxyManager reads HTTP(S)_PROXY / CGI_HTTP_PROXY / no_proxy environment variables; the
// env-dependent setup (without its setUp/tearDown isolation) is not ported.
use bstr::{BStr, BString};
use shirabe::util::Platform;
use shirabe::util::http::ProxyManager;
use shirabe_php_shim::bconcat;

fn set_up() {
    Platform::clear_env("HTTP_PROXY").unwrap();
    Platform::clear_env("http_proxy").unwrap();
    Platform::clear_env("HTTPS_PROXY").unwrap();
    Platform::clear_env("https_proxy").unwrap();
    Platform::clear_env("NO_PROXY").unwrap();
    Platform::clear_env("no_proxy").unwrap();
    Platform::clear_env("CGI_HTTP_PROXY").unwrap();
    Platform::clear_env("cgi_http_proxy").unwrap();
    ProxyManager::reset();
}

fn tear_down() {
    Platform::clear_env("HTTP_PROXY").unwrap();
    Platform::clear_env("http_proxy").unwrap();
    Platform::clear_env("HTTPS_PROXY").unwrap();
    Platform::clear_env("https_proxy").unwrap();
    Platform::clear_env("NO_PROXY").unwrap();
    Platform::clear_env("no_proxy").unwrap();
    Platform::clear_env("CGI_HTTP_PROXY").unwrap();
    Platform::clear_env("cgi_http_proxy").unwrap();
    ProxyManager::reset();
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

#[test]
#[serial_test::serial]
fn test_instantiation() {
    let _tear_down = TearDown;
    set_up();

    // PHP compares object identity (===); the value-based Rust singleton exposes a per-instance
    // generation id instead, which changes only when a new ProxyManager is constructed.
    let original_instance = {
        let guard = ProxyManager::get_instance();
        guard.as_ref().unwrap().__generation()
    };
    let same_instance = {
        let guard = ProxyManager::get_instance();
        guard.as_ref().unwrap().__generation()
    };
    assert_eq!(original_instance, same_instance);

    ProxyManager::reset();
    let new_instance = {
        let guard = ProxyManager::get_instance();
        guard.as_ref().unwrap().__generation()
    };
    assert_ne!(same_instance, new_instance);
}

#[test]
#[serial_test::serial]
fn test_get_proxy_for_request_throws_on_bad_proxy_url() {
    let _tear_down = TearDown;
    set_up();

    Platform::put_env("http_proxy", "localhost").unwrap();
    ProxyManager::reset();
    let guard = ProxyManager::get_instance();
    let proxy_manager = guard.as_ref().unwrap();

    assert!(
        proxy_manager
            .get_proxy_for_request("http://example.com")
            .is_err()
    );
}

#[test]
#[serial_test::serial]
fn test_lowercase_overrides_uppercase() {
    let _tear_down = TearDown;
    set_up();

    // server, url, expectedUrl
    let cases: Vec<(Vec<(&BStr, &BStr)>, &BStr, &BStr)> = vec![
        (
            vec![
                (BStr::new("HTTP_PROXY"), BStr::new("http://upper.com")),
                (BStr::new("http_proxy"), BStr::new("http://lower.com")),
            ],
            BStr::new("http://repo.org"),
            BStr::new("http://lower.com:80"),
        ),
        (
            vec![
                (BStr::new("CGI_HTTP_PROXY"), BStr::new("http://upper.com")),
                (BStr::new("cgi_http_proxy"), BStr::new("http://lower.com")),
            ],
            BStr::new("http://repo.org"),
            BStr::new("http://lower.com:80"),
        ),
        (
            vec![
                (BStr::new("HTTPS_PROXY"), BStr::new("http://upper.com")),
                (BStr::new("https_proxy"), BStr::new("http://lower.com")),
            ],
            BStr::new("https://repo.org"),
            BStr::new("http://lower.com:80"),
        ),
    ];

    for (server, url, expected_url) in cases {
        set_up();
        for (name, value) in &server {
            Platform::put_env(name, value).unwrap();
        }
        ProxyManager::reset();

        let guard = ProxyManager::get_instance();
        let proxy = guard.as_ref().unwrap().get_proxy_for_request(url).unwrap();
        assert_eq!(expected_url, proxy.get_status(None).unwrap());
    }
}

#[test]
#[serial_test::serial]
fn test_cgi_proxy_is_only_used_when_no_http_proxy() {
    let _tear_down = TearDown;
    set_up();

    // server, expectedUrl
    let cases: Vec<(Vec<(&BStr, &BStr)>, &BStr)> = vec![
        (
            vec![(BStr::new("CGI_HTTP_PROXY"), BStr::new("http://cgi.com:80"))],
            BStr::new("http://cgi.com:80"),
        ),
        (
            vec![
                (BStr::new("http_proxy"), BStr::new("http://http.com:80")),
                (BStr::new("CGI_HTTP_PROXY"), BStr::new("http://cgi.com:80")),
            ],
            BStr::new("http://http.com:80"),
        ),
    ];

    for (server, expected_url) in cases {
        set_up();
        for (name, value) in &server {
            Platform::put_env(name, value).unwrap();
        }
        ProxyManager::reset();

        let guard = ProxyManager::get_instance();
        let proxy = guard
            .as_ref()
            .unwrap()
            .get_proxy_for_request("http://repo.org")
            .unwrap();
        assert_eq!(expected_url, proxy.get_status(None).unwrap());
    }
}

#[test]
#[serial_test::serial]
fn test_no_http_proxy_does_not_use_https_proxy() {
    let _tear_down = TearDown;
    set_up();

    Platform::put_env("https_proxy", "https://proxy.com:443").unwrap();
    ProxyManager::reset();
    let guard = ProxyManager::get_instance();
    let proxy = guard
        .as_ref()
        .unwrap()
        .get_proxy_for_request("http://repo.org")
        .unwrap();
    assert_eq!("", proxy.get_status(None).unwrap());
}

#[test]
#[serial_test::serial]
fn test_no_https_proxy_does_not_use_http_proxy() {
    let _tear_down = TearDown;
    set_up();

    Platform::put_env("http_proxy", "http://proxy.com:80").unwrap();
    ProxyManager::reset();
    let guard = ProxyManager::get_instance();
    let proxy = guard
        .as_ref()
        .unwrap()
        .get_proxy_for_request("https://repo.org")
        .unwrap();
    assert_eq!("", proxy.get_status(None).unwrap());
}

#[test]
#[serial_test::serial]
fn test_get_proxy_for_request() {
    use indexmap::IndexMap;
    use shirabe_php_shim::PhpMixed;

    let _tear_down = TearDown;
    set_up();

    let server = vec![
        (
            BStr::new("http_proxy"),
            BStr::new("http://user:p%40ss@proxy.com"),
        ),
        (BStr::new("https_proxy"), BStr::new("https://proxy.com:443")),
        (BStr::new("no_proxy"), BStr::new("other.repo.org")),
    ];

    let http_options =
        |pairs: &[(&BStr, PhpMixed)]| -> Option<IndexMap<BString, IndexMap<BString, PhpMixed>>> {
            let mut http: IndexMap<BString, PhpMixed> = IndexMap::new();
            for (k, v) in pairs {
                http.insert(bconcat!((*k)), v.clone());
            }
            let mut options: IndexMap<BString, IndexMap<BString, PhpMixed>> = IndexMap::new();
            options.insert(BString::from("http"), http);
            Some(options)
        };

    // server, url, options, status, excluded
    let cases: Vec<(
        Vec<(&BStr, &BStr)>,
        &BStr,
        Option<IndexMap<BString, IndexMap<BString, PhpMixed>>>,
        &BStr,
        bool,
    )> = vec![
        (
            vec![],
            BStr::new("http://repo.org"),
            None,
            BStr::new(""),
            false,
        ),
        (
            server.clone(),
            BStr::new("http://repo.org"),
            http_options(&[
                (
                    BStr::new("proxy"),
                    PhpMixed::string(BString::from("tcp://proxy.com:80")),
                ),
                (
                    BStr::new("header"),
                    PhpMixed::string(BString::from("Proxy-Authorization: Basic dXNlcjpwQHNz")),
                ),
                (BStr::new("request_fulluri"), PhpMixed::Bool(true)),
            ]),
            BStr::new("http://***:***@proxy.com:80"),
            false,
        ),
        (
            server.clone(),
            BStr::new("https://repo.org"),
            http_options(&[(
                BStr::new("proxy"),
                PhpMixed::string(BString::from("ssl://proxy.com:443")),
            )]),
            BStr::new("https://proxy.com:443"),
            false,
        ),
        (
            server.clone(),
            BStr::new("https://other.repo.org"),
            None,
            BStr::new("excluded by no_proxy"),
            true,
        ),
    ];

    for (srv, url, options, status, excluded) in cases {
        set_up();
        for (name, value) in &srv {
            Platform::put_env(name, value).unwrap();
        }
        ProxyManager::reset();

        let guard = ProxyManager::get_instance();
        let proxy = guard.as_ref().unwrap().get_proxy_for_request(url).unwrap();

        assert_eq!(options.as_ref(), proxy.get_context_options());
        assert_eq!(status, proxy.get_status(None).unwrap());
        assert_eq!(excluded, proxy.is_excluded_by_no_proxy());
    }
}
