//! ref: composer/tests/Composer/Test/Util/NoProxyPatternTest.php

use bstr::{BStr, BString, ByteSlice};
use shirabe::util::no_proxy_pattern::NoProxyPattern;
use shirabe_php_shim::bconcat;

fn run_test(noproxy: &BStr, url: &BStr, expected: bool) {
    let mut matcher = NoProxyPattern::new(noproxy);
    let url = get_url(url);
    assert_eq!(expected, matcher.test(&url).unwrap());
}

/// Appends a scheme to the test url if it is missing.
fn get_url(url: &BStr) -> BString {
    if url.contains_str("://") {
        return bconcat!(url);
    }

    let mut scheme = BStr::new("http");

    if !url.starts_with(b"[") && url.rfind_byte(b':').is_some() {
        let port = url.split_str(":").nth(1).unwrap_or_default();

        if port == b"443" {
            scheme = BStr::new("https");
        }
    }

    bconcat!(scheme, "://", url)
}

#[test]
fn test_host_name() {
    let noproxy = BStr::new("foobar.com, .barbaz.net");

    run_test(noproxy, BStr::new("foobar.com"), true);
    run_test(noproxy, BStr::new("www.foobar.com"), true);
    run_test(noproxy, BStr::new("foofoobar.com"), false);
    run_test(noproxy, BStr::new("barbaz.net"), true);
    run_test(noproxy, BStr::new("www.barbaz.net"), true);
    run_test(noproxy, BStr::new("barbarbaz.net"), false);
    run_test(noproxy, BStr::new("barbaz.com"), false);
    run_test(noproxy, BStr::new("foobar.com."), false);
}

#[test]
fn test_ip_address() {
    let noproxy = BStr::new("192.168.1.1, 2001:db8::52:0:1");

    run_test(noproxy, BStr::new("192.168.1.1"), true);
    run_test(noproxy, BStr::new("192.168.1.4"), false);
    run_test(noproxy, BStr::new("[2001:db8:0:0:0:52:0:1]"), true);
    run_test(noproxy, BStr::new("[2001:db8:0:0:0:52:0:2]"), false);
    run_test(noproxy, BStr::new("[::FFFF:C0A8:0101]"), true);
    run_test(noproxy, BStr::new("[::FFFF:C0A8:0104]"), false);
}

#[test]
fn test_ip_range() {
    let noproxy = BStr::new("10.0.0.0/30, 2002:db8:a::45/121");

    run_test(noproxy, BStr::new("10.0.0.2"), true);
    run_test(noproxy, BStr::new("10.0.0.4"), false);
    run_test(noproxy, BStr::new("[2002:db8:a:0:0:0:0:7f]"), true);
    run_test(noproxy, BStr::new("[2002:db8:a:0:0:0:0:ff]"), false);
    run_test(noproxy, BStr::new("[::FFFF:0A00:0002]"), true);
    run_test(noproxy, BStr::new("[::FFFF:0A00:0004]"), false);
}

#[test]
fn test_port() {
    let noproxy =
        BStr::new("192.168.1.2:81, 192.168.1.3:80, [2001:db8::52:0:2]:443, [2001:db8::52:0:3]:80");

    run_test(noproxy, BStr::new("192.168.1.3"), true);
    run_test(noproxy, BStr::new("192.168.1.2"), false);
    run_test(noproxy, BStr::new("[2001:db8::52:0:3]"), true);
    run_test(noproxy, BStr::new("[2001:db8::52:0:2]"), false);
}
