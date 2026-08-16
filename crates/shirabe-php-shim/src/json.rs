use crate::PhpMixed;
use indexmap::IndexMap;
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::fmt;

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
    // TODO(php-semantics): other flags (e.g. JSON_HEX_*, JSON_THROW_ON_ERROR) are not handled yet; add
    // them when a call site needs them.
    let mut s = if flags & JSON_PRETTY_PRINT != 0 {
        // PHP's JSON_PRETTY_PRINT uses a 4-space indent.
        let mut buf = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
        value.serialize(&mut ser)?;
        String::from_utf8(buf)?
    } else {
        serde_json::to_string(value)?
    };

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

// PHP's `json_decode($s, true)`: JSON objects decode to associative arrays. Without
// JSON_THROW_ON_ERROR it never throws, returning null on malformed input.
pub fn json_decode_assoc(s: &str) -> anyhow::Result<PhpMixed> {
    json_decode(s, true)
}

// PHP's `json_decode($s, false)`: JSON objects decode to stdClass-equivalent
// `PhpMixed::Object` values. Without JSON_THROW_ON_ERROR it never throws, returning null on
// malformed input.
pub fn json_decode_obj(s: &str) -> anyhow::Result<PhpMixed> {
    json_decode(s, false)
}

fn json_decode(s: &str, assoc: bool) -> anyhow::Result<PhpMixed> {
    let mut deserializer = serde_json::Deserializer::from_str(s);
    let Ok(value) = PhpMixedSeed { assoc }.deserialize(&mut deserializer) else {
        return Ok(PhpMixed::Null);
    };
    if deserializer.end().is_err() {
        return Ok(PhpMixed::Null);
    }
    Ok(value)
}

#[derive(Clone, Copy, Debug)]
struct PhpMixedSeed {
    assoc: bool,
}

impl<'de> DeserializeSeed<'de> for PhpMixedSeed {
    type Value = PhpMixed;

    fn deserialize<D>(self, deserializer: D) -> Result<PhpMixed, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for PhpMixedSeed {
    type Value = PhpMixed;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_unit<E>(self) -> Result<PhpMixed, E> {
        Ok(PhpMixed::Null)
    }

    fn visit_bool<E>(self, v: bool) -> Result<PhpMixed, E> {
        Ok(PhpMixed::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<PhpMixed, E> {
        Ok(PhpMixed::Int(v))
    }

    // Integers beyond i64 and any fractional/exponent number decode to float, matching PHP's
    // default (non-bigint) behaviour.
    fn visit_u64<E>(self, v: u64) -> Result<PhpMixed, E> {
        Ok(match i64::try_from(v) {
            Ok(i) => PhpMixed::Int(i),
            Err(_) => PhpMixed::Float(v as f64),
        })
    }

    fn visit_f64<E>(self, v: f64) -> Result<PhpMixed, E> {
        Ok(PhpMixed::Float(v))
    }

    fn visit_str<E>(self, v: &str) -> Result<PhpMixed, E> {
        Ok(PhpMixed::String(v.to_owned()))
    }

    fn visit_string<E>(self, v: String) -> Result<PhpMixed, E> {
        Ok(PhpMixed::String(v))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<PhpMixed, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(self)? {
            items.push(item);
        }
        Ok(PhpMixed::List(items))
    }

    fn visit_map<A>(self, mut map: A) -> Result<PhpMixed, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut data = IndexMap::new();
        while let Some(key) = map.next_key::<String>()? {
            // A duplicate key keeps its original position and takes the later value, like PHP.
            data.insert(key, map.next_value_seed(self)?);
        }
        Ok(if self.assoc {
            PhpMixed::Array(data)
        } else {
            PhpMixed::Object(data)
        })
    }
}
