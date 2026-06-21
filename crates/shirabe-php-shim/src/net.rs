use std::net::{IpAddr, SocketAddr, ToSocketAddrs};

pub fn gethostname() -> String {
    todo!()
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

pub fn http_get_last_response_headers() -> Option<Vec<String>> {
    todo!()
}

pub fn http_clear_last_response_headers() {
    todo!()
}
