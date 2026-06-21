use crate::PhpMixed;
use indexmap::IndexMap;

pub const PHP_URL_SCHEME: i64 = 0;
pub const PHP_URL_HOST: i64 = 1;
pub const PHP_URL_PORT: i64 = 2;
pub const PHP_URL_USER: i64 = 3;
pub const PHP_URL_PASS: i64 = 4;
pub const PHP_URL_PATH: i64 = 5;
pub const PHP_URL_QUERY: i64 = 6;
pub const PHP_URL_FRAGMENT: i64 = 7;

pub fn parse_url(url: &str, component: i64) -> PhpMixed {
    let all = parse_url_all(url);
    let map = match all.as_array() {
        Some(map) => map,
        // parse_url_all already collapsed a malformed URL to false; propagate it.
        None => return all,
    };
    let key = match component {
        PHP_URL_SCHEME => "scheme",
        PHP_URL_HOST => "host",
        PHP_URL_PORT => "port",
        PHP_URL_USER => "user",
        PHP_URL_PASS => "pass",
        PHP_URL_PATH => "path",
        PHP_URL_QUERY => "query",
        PHP_URL_FRAGMENT => "fragment",
        _ => return PhpMixed::Null,
    };
    map.get(key).cloned().unwrap_or(PhpMixed::Null)
}

pub fn parse_url_all(url: &str) -> PhpMixed {
    // TODO(phase-c): PHP's parse_url uses php_url_parse_ex, which accepts relative
    // and partial URLs and leaves an absent component absent. reqwest::Url
    // (WHATWG/RFC 3986) requires an absolute URL, lowercases the host of special
    // schemes, and normalizes the path (e.g. "http://host" yields path "/"). This
    // is therefore not a byte-for-byte compatible port of parse_url.
    let parsed = match reqwest::Url::parse(url) {
        Ok(parsed) => parsed,
        Err(_) => return PhpMixed::Bool(false),
    };
    let mut map: IndexMap<String, PhpMixed> = IndexMap::new();
    map.insert(
        "scheme".to_string(),
        PhpMixed::String(parsed.scheme().to_string()),
    );
    if let Some(host) = parsed.host_str() {
        map.insert("host".to_string(), PhpMixed::String(host.to_string()));
    }
    if let Some(port) = parsed.port() {
        map.insert("port".to_string(), PhpMixed::Int(port as i64));
    }
    if !parsed.username().is_empty() {
        map.insert(
            "user".to_string(),
            PhpMixed::String(parsed.username().to_string()),
        );
    }
    if let Some(pass) = parsed.password() {
        map.insert("pass".to_string(), PhpMixed::String(pass.to_string()));
    }
    let path = parsed.path();
    if !path.is_empty() {
        map.insert("path".to_string(), PhpMixed::String(path.to_string()));
    }
    if let Some(query) = parsed.query() {
        map.insert("query".to_string(), PhpMixed::String(query.to_string()));
    }
    if let Some(fragment) = parsed.fragment() {
        map.insert(
            "fragment".to_string(),
            PhpMixed::String(fragment.to_string()),
        );
    }
    PhpMixed::Array(map)
}

pub fn http_build_query_mixed(
    data: &IndexMap<String, PhpMixed>,
    numeric_prefix: &str,
    arg_separator: &str,
) -> String {
    let _ = (data, numeric_prefix, arg_separator);
    todo!()
}

pub fn http_build_query(_data: &[(&str, &str)], _sep_str: &str, _sep: &str) -> String {
    todo!()
}
