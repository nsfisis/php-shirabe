use crate::PhpMixed;
use bstr::BString;
use indexmap::IndexMap;
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::fmt;
use std::io::Write as _;

pub trait JsonSerializable {
    fn json_serialize(&self) -> PhpMixed;
}

#[derive(Debug)]
pub struct JsonObject {
    data: IndexMap<BString, PhpMixed>,
}

pub const JSON_UNESCAPED_UNICODE: i64 = 256;
pub const JSON_UNESCAPED_SLASHES: i64 = 64;
pub const JSON_PRETTY_PRINT: i64 = 128;
pub const JSON_THROW_ON_ERROR: i64 = 4194304;
pub const JSON_INVALID_UTF8_IGNORE: i64 = 1048576;

pub fn json_encode<T: serde::Serialize + ?Sized>(value: &T) -> anyhow::Result<BString> {
    // PHP's json_encode() with no flags escapes slashes and non-ASCII characters.
    json_encode_ex(value, 0)
}

pub fn json_encode_ex<T: serde::Serialize + ?Sized>(
    value: &T,
    flags: i64,
) -> anyhow::Result<BString> {
    // serde_json's compact output already matches PHP's `json_encode` with both
    // JSON_UNESCAPED_SLASHES and JSON_UNESCAPED_UNICODE set: forward slashes and non-ASCII
    // characters are emitted verbatim. The two flags below re-apply PHP's default escaping when
    // they are absent.
    // TODO(php-semantics): other flags (e.g. JSON_HEX_*, JSON_THROW_ON_ERROR) are not handled yet; add
    // them when a call site needs them.
    let value =
        crate::PhpJson::with_invalid_utf8_ignore(value, flags & JSON_INVALID_UTF8_IGNORE != 0);
    let mut s = if flags & JSON_PRETTY_PRINT != 0 {
        // PHP's JSON_PRETTY_PRINT uses a 4-space indent.
        let mut buf = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
        serde::Serialize::serialize(&value, &mut ser)?;
        BString::from(buf)
    } else {
        BString::from(serde_json::to_vec(&value)?)
    };

    if flags & JSON_UNESCAPED_SLASHES == 0 {
        s = crate::str_replace("/", "\\/", &s);
    }

    if flags & JSON_UNESCAPED_UNICODE == 0 {
        let mut out = Vec::with_capacity(s.len());
        for c in crate::utf8_text(&s)
            .expect("serde_json writes valid UTF-8")
            .chars()
        {
            if (c as u32) <= 0x7F {
                out.push(c as u8);
            } else {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    write!(out, "\\u{:04x}", unit).expect("writing to a Vec cannot fail");
                }
            }
        }
        s = BString::from(out);
    }

    Ok(s)
}

// PHP's `json_decode($s, true)`: JSON objects decode to associative arrays. Without
// JSON_THROW_ON_ERROR it never throws, returning null on malformed input.
pub fn json_decode_assoc(s: impl AsRef<[u8]>) -> anyhow::Result<PhpMixed> {
    json_decode(s.as_ref(), true)
}

// PHP's `json_decode($s, false)`: JSON objects decode to stdClass-equivalent
// `PhpMixed::Object` values. Without JSON_THROW_ON_ERROR it never throws, returning null on
// malformed input.
pub fn json_decode_obj(s: impl AsRef<[u8]>) -> anyhow::Result<PhpMixed> {
    json_decode(s.as_ref(), false)
}

fn json_decode(s: &[u8], assoc: bool) -> anyhow::Result<PhpMixed> {
    let mut deserializer = serde_json::Deserializer::from_slice(s);
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

    #[expect(clippy::disallowed_types, reason = "imposed by serde::de::Visitor")]
    fn visit_str<E>(self, v: &str) -> Result<PhpMixed, E> {
        Ok(PhpMixed::string(v))
    }

    #[expect(clippy::disallowed_types, reason = "imposed by serde::de::Visitor")]
    fn visit_string<E>(self, v: String) -> Result<PhpMixed, E> {
        Ok(PhpMixed::string(v))
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

    #[expect(
        clippy::disallowed_types,
        reason = "serde deserializes a JSON key as text"
    )]
    fn visit_map<A>(self, mut map: A) -> Result<PhpMixed, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut data = IndexMap::new();
        while let Some(key) = map.next_key::<String>()? {
            // A duplicate key keeps its original position and takes the later value, like PHP.
            data.insert(key.into(), map.next_value_seed(self)?);
        }
        Ok(if self.assoc {
            PhpMixed::Array(data)
        } else {
            PhpMixed::Object(data)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_utf8_ignore_drops_only_the_invalid_bytes() {
        let value = PhpMixed::Array(IndexMap::from([(
            BString::from(b"pa\xfeth".to_vec()),
            PhpMixed::String(BString::from(b"/tmp/caf\xe9".to_vec())),
        )]));
        assert!(json_encode_ex(&value, JSON_UNESCAPED_SLASHES).is_err());
        assert_eq!(
            json_encode_ex(&value, JSON_INVALID_UTF8_IGNORE | JSON_UNESCAPED_SLASHES).unwrap(),
            r#"{"path":"/tmp/caf"}"#
        );
        assert!(json_encode_ex(&value, JSON_UNESCAPED_SLASHES).is_err());
    }

    #[test]
    fn input_that_is_not_utf8_decodes_to_null() {
        assert!(matches!(
            json_decode_assoc(b"\"\xff\"").unwrap(),
            PhpMixed::Null
        ));
        assert!(matches!(
            json_decode_obj(b"{\"a\": \"\xc3\"}").unwrap(),
            PhpMixed::Null
        ));
        assert!(matches!(
            json_decode_assoc(b"\"\xc3\xa9\"").unwrap(),
            PhpMixed::String(s) if s == "é"
        ));
    }
}
