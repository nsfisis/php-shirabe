//! ref: composer/tests/Composer/Test/Util/NoProxyPatternTest.php

use shirabe::util::no_proxy_pattern::NoProxyPattern;

fn run_test(noproxy: &str, url: &str, expected: bool) {
    let mut matcher = NoProxyPattern::new(noproxy);
    let url = get_url(url);
    assert_eq!(expected, matcher.test(&url).unwrap());
}

/// Appends a scheme to the test url if it is missing.
fn get_url(url: &str) -> String {
    if url.contains("://") {
        return url.to_string();
    }

    let mut scheme = "http";

    if !url.starts_with('[') && url.rfind(':').is_some() {
        let port = url.split(':').nth(1).unwrap_or("");

        if port == "443" {
            scheme = "https";
        }
    }

    format!("{}://{}", scheme, url)
}

#[test]
#[ignore = "NoProxyPattern::test reaches a todo!() (substr_count) in the php-shim"]
fn test_host_name() {
    let noproxy = "foobar.com, .barbaz.net";

    run_test(noproxy, "foobar.com", true);
    run_test(noproxy, "www.foobar.com", true);
    run_test(noproxy, "foofoobar.com", false);
    run_test(noproxy, "barbaz.net", true);
    run_test(noproxy, "www.barbaz.net", true);
    run_test(noproxy, "barbarbaz.net", false);
    run_test(noproxy, "barbaz.com", false);
    run_test(noproxy, "foobar.com.", false);
}

#[test]
#[ignore = "NoProxyPattern::test reaches a todo!() (substr_count) in the php-shim"]
fn test_ip_address() {
    let noproxy = "192.168.1.1, 2001:db8::52:0:1";

    run_test(noproxy, "192.168.1.1", true);
    run_test(noproxy, "192.168.1.4", false);
    run_test(noproxy, "[2001:db8:0:0:0:52:0:1]", true);
    run_test(noproxy, "[2001:db8:0:0:0:52:0:2]", false);
    run_test(noproxy, "[::FFFF:C0A8:0101]", true);
    run_test(noproxy, "[::FFFF:C0A8:0104]", false);
}

#[test]
#[ignore = "NoProxyPattern::test reaches a todo!() (substr_count) in the php-shim"]
fn test_ip_range() {
    let noproxy = "10.0.0.0/30, 2002:db8:a::45/121";

    run_test(noproxy, "10.0.0.2", true);
    run_test(noproxy, "10.0.0.4", false);
    run_test(noproxy, "[2002:db8:a:0:0:0:0:7f]", true);
    run_test(noproxy, "[2002:db8:a:0:0:0:0:ff]", false);
    run_test(noproxy, "[::FFFF:0A00:0002]", true);
    run_test(noproxy, "[::FFFF:0A00:0004]", false);
}

#[test]
#[ignore = "NoProxyPattern::test reaches a todo!() (substr_count) in the php-shim"]
fn test_port() {
    let noproxy = "192.168.1.2:81, 192.168.1.3:80, [2001:db8::52:0:2]:443, [2001:db8::52:0:3]:80";

    run_test(noproxy, "192.168.1.3", true);
    run_test(noproxy, "192.168.1.2", false);
    run_test(noproxy, "[2001:db8::52:0:3]", true);
    run_test(noproxy, "[2001:db8::52:0:2]", false);
}
