//! ref: composer/tests/Composer/Test/Util/Http/ProxyItemTest.php

use shirabe::util::http::proxy_item::ProxyItem;

#[test]
#[ignore = "ProxyItem::new reaches a todo!() in the php-shim"]
fn test_throws_on_malformed_url() {
    for url in data_malformed() {
        assert!(ProxyItem::new(url.to_string(), "http_proxy".to_string()).is_err());
    }
}

fn data_malformed() -> Vec<&'static str> {
    vec![
        // 'ws-r'
        "http://user\rname@localhost:80",
        // 'ws-n'
        "http://user\nname@localhost:80",
        // 'ws-t'
        "http://user\tname@localhost:80",
        // 'no-host'
        "localhost",
        // 'no-port'
        "scheme://localhost",
        // 'port-0'
        "http://localhost:0",
        // 'port-big'
        "http://localhost:65536",
    ]
}

#[test]
#[ignore = "ProxyItem::new reaches a todo!() in the php-shim"]
fn test_url_formatting() {
    for (url, expected) in data_formatting() {
        let proxy_item = ProxyItem::new(url.to_string(), "http_proxy".to_string()).unwrap();
        let proxy = proxy_item.to_request_proxy("http".to_string());

        assert_eq!(expected, proxy.get_status(None).unwrap());
    }
}

fn data_formatting() -> Vec<(&'static str, &'static str)> {
    // url, expected
    vec![
        // 'none'
        ("http://proxy.com:8888", "http://proxy.com:8888"),
        // 'lowercases-scheme'
        ("HTTP://proxy.com:8888", "http://proxy.com:8888"),
        // 'adds-http-scheme'
        ("proxy.com:80", "http://proxy.com:80"),
        // 'adds-http-port'
        ("http://proxy.com", "http://proxy.com:80"),
        // 'adds-https-port'
        ("https://proxy.com", "https://proxy.com:443"),
        // 'removes-user'
        ("http://user@proxy.com:6180", "http://***@proxy.com:6180"),
        // 'removes-user-pass'
        ("http://user:p%40ss@proxy.com:6180", "http://***:***@proxy.com:6180"),
    ]
}
