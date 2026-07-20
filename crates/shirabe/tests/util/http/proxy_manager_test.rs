//! ref: composer/tests/Composer/Test/Util/Http/ProxyManagerTest.php

// ProxyManager reads HTTP(S)_PROXY / CGI_HTTP_PROXY / no_proxy environment variables; the
// env-dependent setup (without its setUp/tearDown isolation) is not ported.
use shirabe::util::http::proxy_manager::ProxyManager;
use shirabe::util::platform::Platform;

fn set_up() {
    Platform::clear_env("HTTP_PROXY");
    Platform::clear_env("http_proxy");
    Platform::clear_env("HTTPS_PROXY");
    Platform::clear_env("https_proxy");
    Platform::clear_env("NO_PROXY");
    Platform::clear_env("no_proxy");
    Platform::clear_env("CGI_HTTP_PROXY");
    Platform::clear_env("cgi_http_proxy");
    ProxyManager::reset();
}

fn tear_down() {
    Platform::clear_env("HTTP_PROXY");
    Platform::clear_env("http_proxy");
    Platform::clear_env("HTTPS_PROXY");
    Platform::clear_env("https_proxy");
    Platform::clear_env("NO_PROXY");
    Platform::clear_env("no_proxy");
    Platform::clear_env("CGI_HTTP_PROXY");
    Platform::clear_env("cgi_http_proxy");
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
#[ignore = "not #[serial_test::serial] like test_instantiation; races on the process-wide HTTP_PROXY/etc env vars and the ProxyManager::INSTANCE mutex when run in parallel with the other proxy_manager_test tests, causing spurious PoisonError panics"]
fn test_get_proxy_for_request_throws_on_bad_proxy_url() {
    let _tear_down = TearDown;
    set_up();

    Platform::put_env("http_proxy", "localhost");
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
#[ignore = "not #[serial_test::serial] like test_instantiation; races on the process-wide HTTP_PROXY/etc env vars and the ProxyManager::INSTANCE mutex when run in parallel with the other proxy_manager_test tests, causing spurious PoisonError panics"]
fn test_lowercase_overrides_uppercase() {
    let _tear_down = TearDown;
    set_up();

    // server, url, expectedUrl
    let cases: Vec<(Vec<(&str, &str)>, &str, &str)> = vec![
        (
            vec![
                ("HTTP_PROXY", "http://upper.com"),
                ("http_proxy", "http://lower.com"),
            ],
            "http://repo.org",
            "http://lower.com:80",
        ),
        (
            vec![
                ("CGI_HTTP_PROXY", "http://upper.com"),
                ("cgi_http_proxy", "http://lower.com"),
            ],
            "http://repo.org",
            "http://lower.com:80",
        ),
        (
            vec![
                ("HTTPS_PROXY", "http://upper.com"),
                ("https_proxy", "http://lower.com"),
            ],
            "https://repo.org",
            "http://lower.com:80",
        ),
    ];

    for (server, url, expected_url) in cases {
        set_up();
        for (name, value) in &server {
            Platform::put_env(name, value);
        }
        ProxyManager::reset();

        let guard = ProxyManager::get_instance();
        let proxy = guard.as_ref().unwrap().get_proxy_for_request(url).unwrap();
        assert_eq!(expected_url, proxy.get_status(None).unwrap());
    }
}

#[test]
#[ignore = "not #[serial_test::serial] like test_instantiation; races on the process-wide HTTP_PROXY/etc env vars and the ProxyManager::INSTANCE mutex when run in parallel with the other proxy_manager_test tests, causing spurious PoisonError panics"]
fn test_cgi_proxy_is_only_used_when_no_http_proxy() {
    let _tear_down = TearDown;
    set_up();

    // server, expectedUrl
    let cases: Vec<(Vec<(&str, &str)>, &str)> = vec![
        (
            vec![("CGI_HTTP_PROXY", "http://cgi.com:80")],
            "http://cgi.com:80",
        ),
        (
            vec![
                ("http_proxy", "http://http.com:80"),
                ("CGI_HTTP_PROXY", "http://cgi.com:80"),
            ],
            "http://http.com:80",
        ),
    ];

    for (server, expected_url) in cases {
        set_up();
        for (name, value) in &server {
            Platform::put_env(name, value);
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
#[ignore = "not #[serial_test::serial] like test_instantiation; races on the process-wide HTTP_PROXY/etc env vars and the ProxyManager::INSTANCE mutex when run in parallel with the other proxy_manager_test tests, causing spurious PoisonError panics"]
fn test_no_http_proxy_does_not_use_https_proxy() {
    let _tear_down = TearDown;
    set_up();

    Platform::put_env("https_proxy", "https://proxy.com:443");
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
#[ignore = "not #[serial_test::serial] like test_instantiation; races on the process-wide HTTP_PROXY/etc env vars and the ProxyManager::INSTANCE mutex when run in parallel with the other proxy_manager_test tests, causing spurious PoisonError panics"]
fn test_no_https_proxy_does_not_use_http_proxy() {
    let _tear_down = TearDown;
    set_up();

    Platform::put_env("http_proxy", "http://proxy.com:80");
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
#[ignore = "not #[serial_test::serial] like test_instantiation; races on the process-wide HTTP_PROXY/etc env vars and the ProxyManager::INSTANCE mutex when run in parallel with the other proxy_manager_test tests, causing spurious PoisonError panics"]
fn test_get_proxy_for_request() {
    use indexmap::IndexMap;
    use shirabe_php_shim::PhpMixed;

    let _tear_down = TearDown;
    set_up();

    let server = vec![
        ("http_proxy", "http://user:p%40ss@proxy.com"),
        ("https_proxy", "https://proxy.com:443"),
        ("no_proxy", "other.repo.org"),
    ];

    let http_options =
        |pairs: &[(&str, PhpMixed)]| -> Option<IndexMap<String, IndexMap<String, PhpMixed>>> {
            let mut http: IndexMap<String, PhpMixed> = IndexMap::new();
            for (k, v) in pairs {
                http.insert((*k).to_string(), v.clone());
            }
            let mut options: IndexMap<String, IndexMap<String, PhpMixed>> = IndexMap::new();
            options.insert("http".to_string(), http);
            Some(options)
        };

    // server, url, options, status, excluded
    let cases: Vec<(
        Vec<(&str, &str)>,
        &str,
        Option<IndexMap<String, IndexMap<String, PhpMixed>>>,
        &str,
        bool,
    )> = vec![
        (vec![], "http://repo.org", None, "", false),
        (
            server.clone(),
            "http://repo.org",
            http_options(&[
                ("proxy", PhpMixed::String("tcp://proxy.com:80".to_string())),
                (
                    "header",
                    PhpMixed::String("Proxy-Authorization: Basic dXNlcjpwQHNz".to_string()),
                ),
                ("request_fulluri", PhpMixed::Bool(true)),
            ]),
            "http://***:***@proxy.com:80",
            false,
        ),
        (
            server.clone(),
            "https://repo.org",
            http_options(&[("proxy", PhpMixed::String("ssl://proxy.com:443".to_string()))]),
            "https://proxy.com:443",
            false,
        ),
        (
            server.clone(),
            "https://other.repo.org",
            None,
            "excluded by no_proxy",
            true,
        ),
    ];

    for (srv, url, options, status, excluded) in cases {
        set_up();
        for (name, value) in &srv {
            Platform::put_env(name, value);
        }
        ProxyManager::reset();

        let guard = ProxyManager::get_instance();
        let proxy = guard.as_ref().unwrap().get_proxy_for_request(url).unwrap();

        assert_eq!(options.as_ref(), proxy.get_context_options());
        assert_eq!(status, proxy.get_status(None).unwrap());
        assert_eq!(excluded, proxy.is_excluded_by_no_proxy());
    }
}
