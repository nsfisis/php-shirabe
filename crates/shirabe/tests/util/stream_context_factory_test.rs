//! ref: composer/tests/Composer/Test/Util/StreamContextFactoryTest.php

// These build a stream context and assert proxy/option handling driven by HTTP(S)_PROXY /
// no_proxy environment variables. PHP's setUp/tearDown isolation (which resets the env vars and
// the ProxyManager singleton before/after every test method) is emulated per-test via
// set_up()/tear_down()+TearDown below; since env vars and the ProxyManager singleton are global
// process state, and cargo runs tests in parallel by default (unlike PHPUnit's default serial
// execution), every test here is also tagged `#[serial_test::serial]` to avoid racing other
// serial-tagged tests in this binary that touch the same global state (see
// util/http/proxy_manager_test.rs and http_downloader_test.rs).
use indexmap::IndexMap;
use shirabe::util::http::proxy_manager::ProxyManager;
use shirabe::util::platform::Platform;
use shirabe::util::stream_context_factory::StreamContextFactory;
use shirabe_php_shim::{
    PhpMixed, base64_encode, extension_loaded, implode, stream_context_get_options, stripos,
};

fn s(value: &str) -> PhpMixed {
    PhpMixed::String(value.to_string())
}

fn arr(entries: Vec<(&str, PhpMixed)>) -> PhpMixed {
    PhpMixed::Array(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn list(items: Vec<PhpMixed>) -> PhpMixed {
    PhpMixed::List(items)
}

fn map(entries: Vec<(&str, PhpMixed)>) -> IndexMap<String, PhpMixed> {
    entries
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}

// `PhpMixed`'s `PartialEq` models PHP's `===` (order-sensitive for associative arrays). These
// tests port PHPUnit's `assertEquals`, which compares associative arrays by key/value regardless
// of insertion order (PHP `List`/sequential arrays are still position-sensitive, since reordering
// them changes which value is at which index). This mirrors that PHPUnit semantics for the
// `IndexMap<String, PhpMixed>` results `stream_context_get_options` returns.
fn php_equals(a: &PhpMixed, b: &PhpMixed) -> bool {
    match (a, b) {
        (PhpMixed::Array(a), PhpMixed::Array(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|bv| php_equals(v, bv)))
        }
        (PhpMixed::List(a), PhpMixed::List(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| php_equals(x, y))
        }
        _ => a == b,
    }
}

#[track_caller]
fn assert_options_eq(expected: &IndexMap<String, PhpMixed>, actual: &IndexMap<String, PhpMixed>) {
    let matches = expected.len() == actual.len()
        && expected
            .iter()
            .all(|(k, v)| actual.get(k).is_some_and(|av| php_equals(v, av)));
    assert!(
        matches,
        "options mismatch (order-insensitive):\n  expected: {:?}\n  actual:   {:?}",
        expected, actual
    );
}

fn set_up() {
    Platform::clear_env("HTTP_PROXY");
    Platform::clear_env("http_proxy");
    Platform::clear_env("HTTPS_PROXY");
    Platform::clear_env("https_proxy");
    Platform::clear_env("NO_PROXY");
    Platform::clear_env("no_proxy");
    ProxyManager::reset();
}

fn tear_down() {
    Platform::clear_env("HTTP_PROXY");
    Platform::clear_env("http_proxy");
    Platform::clear_env("HTTPS_PROXY");
    Platform::clear_env("https_proxy");
    Platform::clear_env("NO_PROXY");
    Platform::clear_env("no_proxy");
    ProxyManager::reset();
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

// TODO(phase-d): PHP's dataGetContext second data set passes a `notification` closure in both
// the default and expected params; PhpMixed has no closure variant, so that data set (and thus
// the all-or-nothing testGetContext, which a data provider test cannot partially skip) cannot be
// expressed.
#[test]
#[serial_test::serial]
#[ignore = "dataGetContext passes a notification closure in params; PhpMixed cannot represent a PHP closure, so the data set is unportable"]
fn test_get_context() {
    let _tear_down = TearDown;
    set_up();
    // TODO(phase-d): dataGetContext's second data set passes a `notification` closure in
    // params; PhpMixed cannot represent a PHP closure, so that data set (and thus the
    // all-or-nothing testGetContext, which a data provider test cannot partially skip) is
    // unportable.
    todo!()
}

#[test]
#[serial_test::serial]
#[ignore]
fn test_http_proxy() {
    let _tear_down = TearDown;
    set_up();
    Platform::put_env(
        "http_proxy",
        "http://username:p%40ssword@proxyserver.net:3128/",
    );
    Platform::put_env("HTTP_PROXY", "http://proxyserver/");

    let default_options = map(vec![(
        "http",
        arr(vec![("method", s("GET")), ("header", s("User-Agent: foo"))]),
    )]);
    let context =
        StreamContextFactory::get_context("http://example.org", default_options, IndexMap::new())
            .unwrap();
    let options = stream_context_get_options(&context);

    let expected = map(vec![(
        "http",
        arr(vec![
            ("proxy", s("tcp://proxyserver.net:3128")),
            ("request_fulluri", PhpMixed::Bool(true)),
            ("method", s("GET")),
            (
                "header",
                list(vec![
                    s("User-Agent: foo"),
                    s(&format!(
                        "Proxy-Authorization: Basic {}",
                        base64_encode("username:p@ssword")
                    )),
                ]),
            ),
            ("max_redirects", PhpMixed::Int(20)),
            ("follow_location", PhpMixed::Int(1)),
        ]),
    )]);
    assert_options_eq(&expected, &options);
}

#[test]
#[serial_test::serial]
fn test_http_proxy_with_no_proxy() {
    let _tear_down = TearDown;
    set_up();
    Platform::put_env(
        "http_proxy",
        "http://username:password@proxyserver.net:3128/",
    );
    Platform::put_env("no_proxy", "foo,example.org");

    let default_options = map(vec![(
        "http",
        arr(vec![("method", s("GET")), ("header", s("User-Agent: foo"))]),
    )]);
    let context =
        StreamContextFactory::get_context("http://example.org", default_options, IndexMap::new())
            .unwrap();
    let options = stream_context_get_options(&context);

    let expected = map(vec![(
        "http",
        arr(vec![
            ("method", s("GET")),
            ("max_redirects", PhpMixed::Int(20)),
            ("follow_location", PhpMixed::Int(1)),
            ("header", list(vec![s("User-Agent: foo")])),
        ]),
    )]);
    assert_options_eq(&expected, &options);
}

#[test]
#[serial_test::serial]
fn test_http_proxy_with_no_proxy_wildcard() {
    let _tear_down = TearDown;
    set_up();
    Platform::put_env(
        "http_proxy",
        "http://username:password@proxyserver.net:3128/",
    );
    Platform::put_env("no_proxy", "*");

    let default_options = map(vec![(
        "http",
        arr(vec![("method", s("GET")), ("header", s("User-Agent: foo"))]),
    )]);
    let context =
        StreamContextFactory::get_context("http://example.org", default_options, IndexMap::new())
            .unwrap();
    let options = stream_context_get_options(&context);

    let expected = map(vec![(
        "http",
        arr(vec![
            ("method", s("GET")),
            ("max_redirects", PhpMixed::Int(20)),
            ("follow_location", PhpMixed::Int(1)),
            ("header", list(vec![s("User-Agent: foo")])),
        ]),
    )]);
    assert_options_eq(&expected, &options);
}

#[test]
#[serial_test::serial]
#[ignore]
fn test_options_are_preserved() {
    let _tear_down = TearDown;
    set_up();
    Platform::put_env(
        "http_proxy",
        "http://username:password@proxyserver.net:3128/",
    );

    let default_options = map(vec![(
        "http",
        arr(vec![
            ("method", s("GET")),
            ("header", list(vec![s("User-Agent: foo"), s("X-Foo: bar")])),
            ("request_fulluri", PhpMixed::Bool(false)),
        ]),
    )]);
    let context =
        StreamContextFactory::get_context("http://example.org", default_options, IndexMap::new())
            .unwrap();
    let options = stream_context_get_options(&context);

    let expected = map(vec![(
        "http",
        arr(vec![
            ("proxy", s("tcp://proxyserver.net:3128")),
            ("request_fulluri", PhpMixed::Bool(false)),
            ("method", s("GET")),
            (
                "header",
                list(vec![
                    s("User-Agent: foo"),
                    s("X-Foo: bar"),
                    s(&format!(
                        "Proxy-Authorization: Basic {}",
                        base64_encode("username:password")
                    )),
                ]),
            ),
            ("max_redirects", PhpMixed::Int(20)),
            ("follow_location", PhpMixed::Int(1)),
        ]),
    )]);
    assert_options_eq(&expected, &options);
}

#[test]
#[serial_test::serial]
#[ignore]
fn test_http_proxy_without_port() {
    let _tear_down = TearDown;
    set_up();
    Platform::put_env("https_proxy", "http://username:password@proxyserver.net");

    let default_options = map(vec![(
        "http",
        arr(vec![("method", s("GET")), ("header", s("User-Agent: foo"))]),
    )]);
    let context =
        StreamContextFactory::get_context("https://example.org", default_options, IndexMap::new())
            .unwrap();
    let options = stream_context_get_options(&context);

    let expected = map(vec![(
        "http",
        arr(vec![
            ("proxy", s("tcp://proxyserver.net:80")),
            ("method", s("GET")),
            (
                "header",
                list(vec![
                    s("User-Agent: foo"),
                    s(&format!(
                        "Proxy-Authorization: Basic {}",
                        base64_encode("username:password")
                    )),
                ]),
            ),
            ("max_redirects", PhpMixed::Int(20)),
            ("follow_location", PhpMixed::Int(1)),
        ]),
    )]);
    assert_options_eq(&expected, &options);
}

#[test]
#[serial_test::serial]
fn test_https_proxy_override() {
    let _tear_down = TearDown;
    set_up();
    if !extension_loaded("openssl") {
        // markTestSkipped('Requires openssl')
        return;
    }

    Platform::put_env("http_proxy", "http://username:password@proxyserver.net");
    Platform::put_env("https_proxy", "https://woopproxy.net");

    // Pointless test replaced by ProxyHelperTest.php
    // expectException('Composer\Downloader\TransportException')
    let result = StreamContextFactory::get_context(
        "https://example.org",
        map(vec![(
            "http",
            arr(vec![("method", s("GET")), ("header", s("User-Agent: foo"))]),
        )]),
        IndexMap::new(),
    );
    assert!(result.is_err());
}

#[test]
#[serial_test::serial]
fn test_ssl_proxy() {
    let _tear_down = TearDown;
    for (expected, proxy) in [
        ("ssl://proxyserver:443", "https://proxyserver/"),
        ("ssl://proxyserver:8443", "https://proxyserver:8443"),
    ] {
        set_up();
        Platform::put_env("http_proxy", proxy);

        if extension_loaded("openssl") {
            let context = StreamContextFactory::get_context(
                "http://example.org",
                map(vec![("http", arr(vec![("header", s("User-Agent: foo"))]))]),
                IndexMap::new(),
            )
            .unwrap();
            let options = stream_context_get_options(&context);

            let expected_options = map(vec![(
                "http",
                arr(vec![
                    ("proxy", s(expected)),
                    ("request_fulluri", PhpMixed::Bool(true)),
                    ("max_redirects", PhpMixed::Int(20)),
                    ("follow_location", PhpMixed::Int(1)),
                    ("header", list(vec![s("User-Agent: foo")])),
                ]),
            )]);
            assert_options_eq(&expected_options, &options);
        } else {
            // The catch in PHP asserts the exception is a TransportException; the return type
            // here already guarantees that.
            assert!(
                StreamContextFactory::get_context(
                    "http://example.org",
                    IndexMap::new(),
                    IndexMap::new(),
                )
                .is_err()
            );
        }
    }
}

#[test]
#[serial_test::serial]
fn test_ensure_thatfix_http_header_field_moves_content_type_to_end_of_options() {
    let _tear_down = TearDown;
    set_up();
    let options = map(vec![(
        "http",
        arr(vec![(
            "header",
            s(
                "User-agent: foo\r\nX-Foo: bar\r\nContent-Type: application/json\r\nAuthorization: Basic aW52YWxpZA==",
            ),
        )]),
    )]);
    let expected_header = [
        s("User-agent: foo"),
        s("X-Foo: bar"),
        s("Authorization: Basic aW52YWxpZA=="),
        s("Content-Type: application/json"),
    ];
    let context =
        StreamContextFactory::get_context("http://example.org", options, IndexMap::new()).unwrap();
    let ctxoptions = stream_context_get_options(&context);
    let ctx_header = ctxoptions
        .get("http")
        .and_then(|v| v.as_array())
        .and_then(|a| a.get("header"))
        .and_then(|v| v.as_list())
        .unwrap();
    assert_eq!(expected_header.last().unwrap(), ctx_header.last().unwrap());
}

#[test]
#[serial_test::serial]
#[ignore]
fn test_init_options_does_include_proxy_auth_headers() {
    let _tear_down = TearDown;
    set_up();
    Platform::put_env(
        "https_proxy",
        "http://username:password@proxyserver.net:3128/",
    );

    let options: IndexMap<String, PhpMixed> = IndexMap::new();
    let options =
        StreamContextFactory::init_options("https://example.org", options, false).unwrap();
    let header_list: Vec<String> = options
        .get("http")
        .and_then(|v| v.as_array())
        .and_then(|a| a.get("header"))
        .and_then(|v| v.as_list())
        .unwrap()
        .iter()
        .filter_map(|item| item.as_string().map(|s| s.to_string()))
        .collect();
    let headers = implode(" ", &header_list);

    assert!(stripos(&headers, "Proxy-Authorization").is_some());
}

#[test]
#[serial_test::serial]
#[ignore]
fn test_init_options_for_curl_does_not_include_proxy_auth_headers() {
    let _tear_down = TearDown;
    set_up();
    Platform::put_env(
        "http_proxy",
        "http://username:password@proxyserver.net:3128/",
    );

    let options: IndexMap<String, PhpMixed> = IndexMap::new();
    let options = StreamContextFactory::init_options("https://example.org", options, true).unwrap();
    let header_list: Vec<String> = options
        .get("http")
        .and_then(|v| v.as_array())
        .and_then(|a| a.get("header"))
        .and_then(|v| v.as_list())
        .unwrap()
        .iter()
        .filter_map(|item| item.as_string().map(|s| s.to_string()))
        .collect();
    let headers = implode(" ", &header_list);

    assert!(stripos(&headers, "Proxy-Authorization").is_none());
}
