use std::net::{IpAddr, SocketAddr, ToSocketAddrs};

pub fn gethostname() -> String {
    // PHP's gethostname() wraps gethostname(2). On Linux the kernel exposes the
    // same value via /proc; fall back to the HOSTNAME env var, then to "localhost".
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "localhost".to_string())
}

// Resolves the first IPv4 address for the host name, mirroring PHP's gethostbyname
// which only ever yields an IPv4 record and returns the unmodified host name on
// failure.
pub fn gethostbyname(hostname: &str) -> String {
    match (hostname, 0u16).to_socket_addrs() {
        Ok(addrs) => addrs
            .filter_map(|addr| match addr {
                SocketAddr::V4(v4) => Some(v4.ip().to_string()),
                SocketAddr::V6(_) => None,
            })
            .next()
            .unwrap_or_else(|| hostname.to_string()),
        Err(_) => hostname.to_string(),
    }
}

pub fn inet_pton(host: &str) -> Option<Vec<u8>> {
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => Some(v4.octets().to_vec()),
        Ok(IpAddr::V6(v6)) => Some(v6.octets().to_vec()),
        Err(_) => None,
    }
}

thread_local! {
    // PHP records the response headers of the most recent HTTP stream wrapper request
    // (the engine-side storage behind `$http_response_header` and, since PHP 8.4, the
    // http_get_last_response_headers()/http_clear_last_response_headers() pair).
    static LAST_RESPONSE_HEADERS: std::cell::RefCell<Option<Vec<String>>> =
        const { std::cell::RefCell::new(None) };
}

// Engine-side hook with no PHP userland counterpart: the HTTP stream layer must call this
// after each request, like PHP's http wrapper populating `$http_response_header`. No stream
// layer performs HTTP requests yet (see the TODO(phase-c) in util/remote_filesystem.rs), so
// until then the store stays empty and the getter below returns None, which matches PHP
// before any HTTP stream request was made.
pub fn http_record_last_response_headers(headers: Vec<String>) {
    LAST_RESPONSE_HEADERS.with(|h| *h.borrow_mut() = Some(headers));
}

pub fn http_get_last_response_headers() -> Option<Vec<String>> {
    LAST_RESPONSE_HEADERS.with(|h| h.borrow().clone())
}

pub fn http_clear_last_response_headers() {
    LAST_RESPONSE_HEADERS.with(|h| *h.borrow_mut() = None);
}
