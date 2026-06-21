//! ref: composer/tests/Composer/Test/Util/StreamContextFactoryTest.php

// These build a stream context and assert proxy/option handling driven by HTTP(S)_PROXY /
// no_proxy environment variables; the env-dependent setup (without its setUp/tearDown
// isolation) is not ported.
use indexmap::IndexMap;
use shirabe::util::http::proxy_manager::ProxyManager;
use shirabe::util::platform::Platform;
use shirabe::util::stream_context_factory::StreamContextFactory;
use shirabe_php_shim::{PhpMixed, implode, stripos};

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

#[test]
#[ignore = "stream_context_get_options/stream_context_get_params shim functions do not exist; cannot read back created context to assert"]
fn test_get_context() {
    let _tear_down = TearDown;
    set_up();
    todo!()
}

#[test]
#[ignore = "stream_context_get_options shim function does not exist; cannot read back created context to assert"]
fn test_http_proxy() {
    let _tear_down = TearDown;
    set_up();
    todo!()
}

#[test]
#[ignore = "stream_context_get_options shim function does not exist; cannot read back created context to assert"]
fn test_http_proxy_with_no_proxy() {
    let _tear_down = TearDown;
    set_up();
    todo!()
}

#[test]
#[ignore = "stream_context_get_options shim function does not exist; cannot read back created context to assert"]
fn test_http_proxy_with_no_proxy_wildcard() {
    let _tear_down = TearDown;
    set_up();
    todo!()
}

#[test]
#[ignore = "stream_context_get_options shim function does not exist; cannot read back created context to assert"]
fn test_options_are_preserved() {
    let _tear_down = TearDown;
    set_up();
    todo!()
}

#[test]
#[ignore = "stream_context_get_options shim function does not exist; cannot read back created context to assert"]
fn test_http_proxy_without_port() {
    let _tear_down = TearDown;
    set_up();
    todo!()
}

#[test]
#[ignore = "stream_context_get_options shim function does not exist; cannot read back created context to assert"]
fn test_https_proxy_override() {
    let _tear_down = TearDown;
    set_up();
    todo!()
}

#[test]
#[ignore = "stream_context_get_options shim function does not exist; cannot read back created context to assert"]
fn test_ssl_proxy() {
    let _tear_down = TearDown;
    set_up();
    todo!()
}

#[test]
#[ignore = "stream_context_get_options shim function does not exist; cannot read back created context to assert"]
fn test_ensure_thatfix_http_header_field_moves_content_type_to_end_of_options() {
    let _tear_down = TearDown;
    set_up();
    todo!()
}

#[test]
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
