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
#[ignore = "not yet ported (ProxyManager is driven by proxy environment variables)"]
fn test_instantiation() {
    let _tear_down = TearDown;
    set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (ProxyManager is driven by proxy environment variables)"]
fn test_get_proxy_for_request_throws_on_bad_proxy_url() {
    let _tear_down = TearDown;
    set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (ProxyManager is driven by proxy environment variables)"]
fn test_lowercase_overrides_uppercase() {
    let _tear_down = TearDown;
    set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (ProxyManager is driven by proxy environment variables)"]
fn test_cgi_proxy_is_only_used_when_no_http_proxy() {
    let _tear_down = TearDown;
    set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (ProxyManager is driven by proxy environment variables)"]
fn test_no_http_proxy_does_not_use_https_proxy() {
    let _tear_down = TearDown;
    set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (ProxyManager is driven by proxy environment variables)"]
fn test_no_https_proxy_does_not_use_http_proxy() {
    let _tear_down = TearDown;
    set_up();
    todo!()
}

#[test]
#[ignore = "not yet ported (ProxyManager is driven by proxy environment variables)"]
fn test_get_proxy_for_request() {
    let _tear_down = TearDown;
    set_up();
    todo!()
}
