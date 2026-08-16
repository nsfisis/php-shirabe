use crate::PhpMixed;
use indexmap::IndexMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UrlComponents {
    pub scheme: Option<String>,
    pub host: Option<String>,
    pub port: Option<i64>,
    pub user: Option<String>,
    pub pass: Option<String>,
    pub path: Option<String>,
    pub query: Option<String>,
    pub fragment: Option<String>,
}

/// PHP `parse_url()`. `None` for an invalid URL.
pub fn parse_url(url: &str) -> Option<UrlComponents> {
    // TODO(php-semantics): PHP's parse_url uses php_url_parse_ex, which accepts relative
    // and partial URLs and leaves an absent component absent. reqwest::Url
    // (WHATWG/RFC 3986) requires an absolute URL, lowercases the host of special
    // schemes, and normalizes the path (e.g. "http://host" yields path "/"). This
    // is therefore not a byte-for-byte compatible port of parse_url.
    let parsed = reqwest::Url::parse(url).ok()?;
    Some(UrlComponents {
        scheme: Some(parsed.scheme().to_string()),
        host: parsed.host_str().map(str::to_string),
        port: parsed.port().map(i64::from),
        user: Some(parsed.username())
            .filter(|user| !user.is_empty())
            .map(str::to_string),
        pass: parsed.password().map(str::to_string),
        path: Some(parsed.path())
            .filter(|path| !path.is_empty())
            .map(str::to_string),
        query: parsed.query().map(str::to_string),
        fragment: parsed.fragment().map(str::to_string),
    })
}

pub fn http_build_query_mixed(
    data: &IndexMap<String, PhpMixed>,
    numeric_prefix: &str,
    arg_separator: &str,
) -> String {
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (key, value) in data {
        // numeric_prefix is prepended only to integer keys at the top level.
        let key = if key.parse::<i64>().is_ok() {
            format!("{numeric_prefix}{key}")
        } else {
            key.clone()
        };
        append_query_pairs(&mut pairs, key, value);
    }
    encode_pairs(&pairs, arg_separator)
}

pub fn http_build_query(data: &[(&str, &str)]) -> String {
    encode_pairs(data, "&")
}

fn encode_pairs<T: serde::Serialize>(pairs: T, arg_separator: &str) -> String {
    let encoded = serde_urlencoded::to_string(pairs).unwrap();
    if arg_separator == "&" {
        encoded
    } else {
        // serde_urlencoded percent-encodes any literal '&' in keys/values, so the
        // only remaining '&' are the separators we are replacing.
        encoded.replace('&', arg_separator)
    }
}

fn append_query_pairs(pairs: &mut Vec<(String, String)>, key: String, value: &PhpMixed) {
    match value {
        // Null values are omitted from the query string entirely.
        PhpMixed::Null => {}
        PhpMixed::Bool(b) => pairs.push((key, if *b { "1" } else { "0" }.to_string())),
        PhpMixed::Int(i) => pairs.push((key, i.to_string())),
        PhpMixed::Float(f) => pairs.push((key, f.to_string())),
        PhpMixed::String(s) => pairs.push((key, s.clone())),
        PhpMixed::List(items) => {
            for (i, item) in items.iter().enumerate() {
                append_query_pairs(pairs, format!("{key}[{i}]"), item);
            }
        }
        PhpMixed::Array(map) | PhpMixed::Object(map) => {
            for (k, v) in map {
                append_query_pairs(pairs, format!("{key}[{k}]"), v);
            }
        }
    }
}
