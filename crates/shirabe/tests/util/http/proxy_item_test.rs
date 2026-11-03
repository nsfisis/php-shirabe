//! ref: composer/tests/Composer/Test/Util/Http/ProxyItemTest.php

use bstr::{BStr, BString};
use shirabe::util::http::proxy_item::ProxyItem;

#[test]
fn test_throws_on_malformed_url() {
    for url in data_malformed() {
        assert!(ProxyItem::new(url, BString::from("http_proxy")).is_err());
    }
}

fn data_malformed() -> Vec<&'static BStr> {
    vec![
        // 'ws-r'
        BStr::new("http://user\rname@localhost:80"),
        // 'ws-n'
        BStr::new("http://user\nname@localhost:80"),
        // 'ws-t'
        BStr::new("http://user\tname@localhost:80"),
        // 'no-host'
        BStr::new("localhost"),
        // 'no-port'
        BStr::new("scheme://localhost"),
        // 'port-0'
        BStr::new("http://localhost:0"),
        // 'port-big'
        BStr::new("http://localhost:65536"),
    ]
}

#[test]
fn test_url_formatting() {
    for (url, expected) in data_formatting() {
        let proxy_item = ProxyItem::new(url, BString::from("http_proxy")).unwrap();
        let proxy = proxy_item.to_request_proxy("http");

        assert_eq!(expected, proxy.get_status(None).unwrap());
    }
}

fn data_formatting() -> Vec<(&'static BStr, &'static BStr)> {
    // url, expected
    vec![
        // 'none'
        (
            BStr::new("http://proxy.com:8888"),
            BStr::new("http://proxy.com:8888"),
        ),
        // 'lowercases-scheme'
        (
            BStr::new("HTTP://proxy.com:8888"),
            BStr::new("http://proxy.com:8888"),
        ),
        // 'adds-http-scheme'
        (BStr::new("proxy.com:80"), BStr::new("http://proxy.com:80")),
        // 'adds-http-port'
        (
            BStr::new("http://proxy.com"),
            BStr::new("http://proxy.com:80"),
        ),
        // 'adds-https-port'
        (
            BStr::new("https://proxy.com"),
            BStr::new("https://proxy.com:443"),
        ),
        // 'removes-user'
        (
            BStr::new("http://user@proxy.com:6180"),
            BStr::new("http://***@proxy.com:6180"),
        ),
        // 'removes-user-pass'
        (
            BStr::new("http://user:p%40ss@proxy.com:6180"),
            BStr::new("http://***:***@proxy.com:6180"),
        ),
    ]
}
