use crate::PhpMixed;
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_src::standard::url::php_url_parse_ex2;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UrlComponents {
    pub scheme: Option<BString>,
    pub host: Option<BString>,
    pub port: Option<i64>,
    pub user: Option<BString>,
    pub pass: Option<BString>,
    pub path: Option<BString>,
    pub query: Option<BString>,
    pub fragment: Option<BString>,
}

/// PHP `parse_url()`. `None` for a seriously malformed URL.
pub fn parse_url(url: impl AsRef<[u8]>) -> Option<UrlComponents> {
    let parsed = php_url_parse_ex2(url.as_ref())?;
    Some(UrlComponents {
        scheme: parsed.scheme,
        host: parsed.host,
        port: parsed.port.map(i64::from),
        user: parsed.user,
        pass: parsed.pass,
        path: parsed.path,
        query: parsed.query,
        fragment: parsed.fragment,
    })
}

pub fn http_build_query_mixed(
    data: &IndexMap<BString, PhpMixed>,
    numeric_prefix: impl AsRef<[u8]>,
    arg_separator: impl AsRef<[u8]>,
) -> BString {
    let mut pairs: Vec<(BString, BString)> = Vec::new();
    for (key, value) in data {
        // numeric_prefix is prepended only to integer keys at the top level.
        let key = if crate::canonical_int_key(key).is_some() {
            crate::bconcat!(numeric_prefix.as_ref(), key)
        } else {
            key.clone()
        };
        append_query_pairs(&mut pairs, key, value);
    }
    encode_pairs(&pairs, arg_separator.as_ref())
}

pub fn http_build_query<K: AsRef<[u8]>, V: AsRef<[u8]>>(data: &[(K, V)]) -> BString {
    let pairs: Vec<(BString, BString)> = data
        .iter()
        .map(|(key, value)| (BString::from(key.as_ref()), BString::from(value.as_ref())))
        .collect();
    encode_pairs(&pairs, b"&")
}

// PHP's default PHP_QUERY_RFC1738 encoding: each key and value is encoded with urlencode().
fn encode_pairs(pairs: &[(BString, BString)], arg_separator: &[u8]) -> BString {
    let encoded: Vec<BString> = pairs
        .iter()
        .map(|(key, value)| crate::bconcat!(crate::urlencode(key), "=", crate::urlencode(value)))
        .collect();
    crate::implode(arg_separator, &encoded)
}

fn append_query_pairs(pairs: &mut Vec<(BString, BString)>, key: BString, value: &PhpMixed) {
    match value {
        // Null values are omitted from the query string entirely.
        PhpMixed::Null => {}
        PhpMixed::Bool(b) => pairs.push((key, BString::from(if *b { "1" } else { "0" }))),
        PhpMixed::Int(i) => pairs.push((key, BString::from(i.to_string()))),
        PhpMixed::Float(f) => pairs.push((key, BString::from(f.to_string()))),
        PhpMixed::String(s) => pairs.push((key, s.clone())),
        PhpMixed::List(items) => {
            for (i, item) in items.iter().enumerate() {
                append_query_pairs(pairs, crate::bconcat!(key, "[", i as i64, "]"), item);
            }
        }
        PhpMixed::Array(map) | PhpMixed::Object(map) => {
            for (k, v) in map {
                append_query_pairs(pairs, crate::bconcat!(key, "[", k, "]"), v);
            }
        }
    }
}
