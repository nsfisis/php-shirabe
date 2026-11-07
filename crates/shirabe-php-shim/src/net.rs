use crate::{display_to_bytes, getenv, os_str_from_bytes, trim, utf8_text};
use bstr::BString;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};

pub fn gethostname() -> std::ffi::OsString {
    // PHP's gethostname() wraps gethostname(2). On Linux the kernel exposes the
    // same value via /proc; fall back to the HOSTNAME env var, then to "localhost".
    std::fs::read("/proc/sys/kernel/hostname")
        .ok()
        .map(trim)
        .filter(|s| !s.is_empty())
        .map(|s| os_str_from_bytes(&s).to_os_string())
        .or_else(|| getenv("HOSTNAME").filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "localhost".into())
}

// Resolves the first IPv4 address for the host name, mirroring PHP's gethostbyname
// which only ever yields an IPv4 record and returns the unmodified host name on
// failure.
pub fn gethostbyname(hostname: impl AsRef<[u8]>) -> BString {
    let hostname = hostname.as_ref();
    // A host name that is not UTF-8 resolves to nothing.
    let Some(text) = utf8_text(hostname) else {
        return BString::from(hostname);
    };
    match (text, 0u16).to_socket_addrs() {
        Ok(addrs) => addrs
            .filter_map(|addr| match addr {
                SocketAddr::V4(v4) => Some(display_to_bytes(v4.ip())),
                SocketAddr::V6(_) => None,
            })
            .next()
            .unwrap_or_else(|| BString::from(hostname)),
        Err(_) => BString::from(hostname),
    }
}

pub fn inet_pton(host: impl AsRef<[u8]>) -> Option<Vec<u8>> {
    // Every address PHP accepts is ASCII.
    match utf8_text(host.as_ref())?.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => Some(v4.octets().to_vec()),
        Ok(IpAddr::V6(v6)) => Some(v6.octets().to_vec()),
        Err(_) => None,
    }
}

thread_local! {
    // PHP records the response headers of the most recent HTTP stream wrapper request
    // (the engine-side storage behind `$http_response_header` and, since PHP 8.4, the
    // http_get_last_response_headers()/http_clear_last_response_headers() pair).
    static LAST_RESPONSE_HEADERS: std::cell::RefCell<Option<Vec<BString>>> =
        const { std::cell::RefCell::new(None) };
}

// Engine-side hook with no PHP userland counterpart: the HTTP stream layer must call this
// after each request, like PHP's http wrapper populating `$http_response_header`. No stream
// layer on this side performs HTTP requests, so the store stays empty and the getter below
// returns None, which matches PHP before any HTTP stream request was made.
pub fn http_record_last_response_headers(headers: Vec<BString>) {
    LAST_RESPONSE_HEADERS.with(|h| *h.borrow_mut() = Some(headers));
}

pub fn http_get_last_response_headers() -> Option<Vec<BString>> {
    LAST_RESPONSE_HEADERS.with(|h| h.borrow().clone())
}

pub fn http_clear_last_response_headers() {
    LAST_RESPONSE_HEADERS.with(|h| *h.borrow_mut() = None);
}
