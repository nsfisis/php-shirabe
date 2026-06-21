use crate::ArrayObject;
use crate::PhpMixed;
use indexmap::IndexMap;

pub trait JsonSerializable {
    fn json_serialize(&self) -> PhpMixed;
}

#[derive(Debug)]
pub struct JsonObject {
    data: IndexMap<String, PhpMixed>,
}

pub const JSON_UNESCAPED_UNICODE: i64 = 256;
pub const JSON_UNESCAPED_SLASHES: i64 = 64;
pub const JSON_PRETTY_PRINT: i64 = 128;
pub const JSON_THROW_ON_ERROR: i64 = 4194304;
pub const JSON_INVALID_UTF8_IGNORE: i64 = 1048576;

pub fn json_encode<T: serde::Serialize + ?Sized>(value: &T) -> anyhow::Result<String> {
    // PHP's json_encode() with no flags escapes slashes and non-ASCII characters.
    json_encode_ex(value, 0)
}

pub fn json_encode_ex<T: serde::Serialize + ?Sized>(
    value: &T,
    flags: i64,
) -> anyhow::Result<String> {
    // serde_json's compact output already matches PHP's `json_encode` with both
    // JSON_UNESCAPED_SLASHES and JSON_UNESCAPED_UNICODE set: forward slashes and non-ASCII
    // characters are emitted verbatim. The two flags below re-apply PHP's default escaping when
    // they are absent.
    // TODO(phase-c): other flags (e.g. JSON_PRETTY_PRINT, JSON_HEX_*, JSON_THROW_ON_ERROR) are not
    // handled yet; add them when a call site needs them.
    let mut s = serde_json::to_string(value)?;

    if flags & JSON_UNESCAPED_SLASHES == 0 {
        s = s.replace('/', "\\/");
    }

    if flags & JSON_UNESCAPED_UNICODE == 0 {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            if (c as u32) <= 0x7F {
                out.push(c);
            } else {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{:04x}", unit));
                }
            }
        }
        s = out;
    }

    Ok(s)
}

// PHP's two-argument `json_decode`: without JSON_THROW_ON_ERROR it never throws,
// returning null on malformed input. With `assoc` false, JSON objects decode to
// stdClass-equivalent ArrayObject values; with `assoc` true, to associative arrays.
pub fn json_decode(s: &str, assoc: bool) -> anyhow::Result<PhpMixed> {
    match serde_json::from_str::<serde_json::Value>(s) {
        Ok(value) => Ok(json_value_to_php_mixed(value, assoc)),
        Err(_) => Ok(PhpMixed::Null),
    }
}

fn json_value_to_php_mixed(value: serde_json::Value, assoc: bool) -> PhpMixed {
    match value {
        serde_json::Value::Null => PhpMixed::Null,
        serde_json::Value::Bool(b) => PhpMixed::Bool(b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => PhpMixed::Int(i),
            // Integers beyond i64 and any fractional/exponent number decode to float,
            // matching PHP's default (non-bigint) behaviour.
            None => PhpMixed::Float(n.as_f64().unwrap_or(0.0)),
        },
        serde_json::Value::String(s) => PhpMixed::String(s),
        serde_json::Value::Array(items) => PhpMixed::List(
            items
                .into_iter()
                .map(|item| json_value_to_php_mixed(item, assoc))
                .collect(),
        ),
        serde_json::Value::Object(entries) => {
            let data: IndexMap<String, PhpMixed> = entries
                .into_iter()
                .map(|(k, v)| (k, json_value_to_php_mixed(v, assoc)))
                .collect();
            if assoc {
                PhpMixed::Array(data)
            } else {
                PhpMixed::Object(ArrayObject {
                    data: data.into_iter().collect(),
                })
            }
        }
    }
}
