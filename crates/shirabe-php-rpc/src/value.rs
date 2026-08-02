//! Plugin-boundary value model and its wire codec. The wire format is documented in
//! `docs/dev/php-rpc.md`.
//!
//! `PluginValue` is the value type crossing the Rust/PHP RPC boundary. It is deliberately
//! independent from `shirabe_php_shim::PhpMixed`: handles exist only at the plugin boundary,
//! and the codec below is a separate implementation from `shirabe_php_shim::var::serialize`.
//!
//! Strings and array keys are byte strings (`Vec<u8>`), matching PHP's string semantics: the
//! codec must round-trip non-UTF-8 byte sequences losslessly.

use anyhow::bail;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// A handle to an object whose entity lives on the Rust side. PHP holds a thin proxy stub.
#[derive(Debug, Clone, PartialEq)]
pub struct RustObjHandle {
    pub rhandle: u64,
    pub class: String,
    pub epoch: u64,
    /// Present when the descriptor also carries a value snapshot of the entity's fields.
    pub snapshot: Option<IndexMap<Vec<u8>, PluginValue>>,
}

/// A handle to an object whose entity lives in the PHP child process.
#[derive(Debug, Clone, PartialEq)]
pub struct PhpObjHandle {
    pub phandle: u64,
    pub class: String,
    pub implements: Vec<String>,
}

/// A PHP class (not an instance), identified by its fully qualified name.
#[derive(Debug, Clone, PartialEq)]
pub struct PhpClassHandle {
    pub class: String,
}

/// The value model of the plugin RPC boundary: PHP scalars, arrays, and handle descriptors.
///
/// `Object` is encode-only: the wire representation of a PHP array does not distinguish arrays
/// from objects, so the decoder only ever produces `List` (contiguous 0-based int keys) or
/// `Array`. An encoded `Object` lands on the PHP side as a plain array.
#[derive(Debug, Clone, PartialEq)]
pub enum PluginValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Vec<u8>),
    List(Vec<PluginValue>),
    Array(IndexMap<Vec<u8>, PluginValue>),
    Object(IndexMap<Vec<u8>, PluginValue>),
    RustHandle(RustObjHandle),
    PhpHandle(PhpObjHandle),
    PhpClass(PhpClassHandle),
}

impl PluginValue {
    pub fn string(s: impl Into<String>) -> PluginValue {
        PluginValue::String(s.into().into_bytes())
    }

    /// Converts a plain data value (no handles are ever produced) coming from ported code.
    pub fn from_php_mixed(value: &PhpMixed) -> PluginValue {
        match value {
            PhpMixed::Null => PluginValue::Null,
            PhpMixed::Bool(b) => PluginValue::Bool(*b),
            PhpMixed::Int(n) => PluginValue::Int(*n),
            PhpMixed::Float(f) => PluginValue::Float(*f),
            PhpMixed::String(s) => PluginValue::String(s.clone().into_bytes()),
            PhpMixed::List(items) => {
                PluginValue::List(items.iter().map(PluginValue::from_php_mixed).collect())
            }
            PhpMixed::Array(map) => PluginValue::Array(
                map.iter()
                    .map(|(k, v)| (k.clone().into_bytes(), PluginValue::from_php_mixed(v)))
                    .collect(),
            ),
            PhpMixed::Object(map) => PluginValue::Object(
                map.iter()
                    .map(|(k, v)| (k.clone().into_bytes(), PluginValue::from_php_mixed(v)))
                    .collect(),
            ),
        }
    }

    /// Converts back to `PhpMixed` for callers outside the plugin boundary. Handles have no
    /// `PhpMixed` counterpart and fail. Non-UTF-8 bytes are replaced, matching how the previous
    /// scalar-only response parser exposed PHP strings to `PhpMixed` consumers.
    pub fn to_php_mixed(&self) -> anyhow::Result<PhpMixed> {
        Ok(match self {
            PluginValue::Null => PhpMixed::Null,
            PluginValue::Bool(b) => PhpMixed::Bool(*b),
            PluginValue::Int(n) => PhpMixed::Int(*n),
            PluginValue::Float(f) => PhpMixed::Float(*f),
            PluginValue::String(bytes) => {
                PhpMixed::String(String::from_utf8_lossy(bytes).into_owned())
            }
            PluginValue::List(items) => PhpMixed::List(
                items
                    .iter()
                    .map(PluginValue::to_php_mixed)
                    .collect::<anyhow::Result<_>>()?,
            ),
            PluginValue::Array(map) | PluginValue::Object(map) => PhpMixed::Array(
                map.iter()
                    .map(|(k, v)| Ok((String::from_utf8_lossy(k).into_owned(), v.to_php_mixed()?)))
                    .collect::<anyhow::Result<_>>()?,
            ),
            PluginValue::RustHandle(_) | PluginValue::PhpHandle(_) | PluginValue::PhpClass(_) => {
                bail!("a handle descriptor cannot be represented as PhpMixed: {self:?}")
            }
        })
    }
}

/// Maximum nesting depth the decoder accepts before rejecting the payload, so a corrupted or
/// hostile payload cannot overflow the stack.
pub const MAX_DECODE_DEPTH: usize = 512;

const RUST_HANDLE_KEY: &[u8] = b"__rhandle";
const CLASS_KEY: &[u8] = b"__class";
const EPOCH_KEY: &[u8] = b"__epoch";
const SNAPSHOT_KEY: &[u8] = b"__snapshot";
const PHP_HANDLE_KEY: &[u8] = b"__phandle";
const IMPLEMENTS_KEY: &[u8] = b"__implements";
const PHP_CLASS_KEY: &[u8] = b"__pclass";

/// Encodes a `PluginValue` in PHP `serialize()` grammar, byte-compatible with what the PHP core
/// implementation produces under `serialize_precision=-1`.
pub fn serialize(value: &PluginValue) -> Vec<u8> {
    let mut out = Vec::new();
    serialize_into(value, &mut out);
    out
}

fn serialize_into(value: &PluginValue, out: &mut Vec<u8>) {
    match value {
        PluginValue::Null => out.extend_from_slice(b"N;"),
        PluginValue::Bool(b) => {
            out.extend_from_slice(if *b { b"b:1;" } else { b"b:0;" });
        }
        PluginValue::Int(n) => {
            out.extend_from_slice(b"i:");
            out.extend_from_slice(n.to_string().as_bytes());
            out.push(b';');
        }
        PluginValue::Float(f) => {
            out.extend_from_slice(b"d:");
            let mut repr = String::new();
            shirabe_php_src::zend::zend_smart_str::smart_str_append_double(
                &mut repr, *f, -1, false,
            );
            out.extend_from_slice(repr.as_bytes());
            out.push(b';');
        }
        PluginValue::String(bytes) => serialize_bytes(bytes, out),
        PluginValue::List(items) => {
            out.extend_from_slice(b"a:");
            out.extend_from_slice(items.len().to_string().as_bytes());
            out.extend_from_slice(b":{");
            for (index, item) in items.iter().enumerate() {
                out.extend_from_slice(b"i:");
                out.extend_from_slice(index.to_string().as_bytes());
                out.push(b';');
                serialize_into(item, out);
            }
            out.push(b'}');
        }
        // An object lands on the PHP side as a plain array: `allowed_classes: false` bans `O:`
        // records from the wire, so `Object` is a write-only label (see docs/dev/php-rpc.md).
        PluginValue::Array(map) | PluginValue::Object(map) => serialize_map(map, out),
        PluginValue::RustHandle(handle) => {
            let mut map: IndexMap<Vec<u8>, PluginValue> = IndexMap::new();
            map.insert(
                RUST_HANDLE_KEY.to_vec(),
                PluginValue::Int(i64::try_from(handle.rhandle).expect("rhandle exceeds i64")),
            );
            map.insert(
                CLASS_KEY.to_vec(),
                PluginValue::string(handle.class.clone()),
            );
            map.insert(
                EPOCH_KEY.to_vec(),
                PluginValue::Int(i64::try_from(handle.epoch).expect("epoch exceeds i64")),
            );
            if let Some(snapshot) = &handle.snapshot {
                map.insert(SNAPSHOT_KEY.to_vec(), PluginValue::Array(snapshot.clone()));
            }
            serialize_map(&map, out);
        }
        PluginValue::PhpHandle(handle) => {
            let mut map: IndexMap<Vec<u8>, PluginValue> = IndexMap::new();
            map.insert(
                PHP_HANDLE_KEY.to_vec(),
                PluginValue::Int(i64::try_from(handle.phandle).expect("phandle exceeds i64")),
            );
            map.insert(
                CLASS_KEY.to_vec(),
                PluginValue::string(handle.class.clone()),
            );
            map.insert(
                IMPLEMENTS_KEY.to_vec(),
                PluginValue::List(
                    handle
                        .implements
                        .iter()
                        .map(|name| PluginValue::string(name.clone()))
                        .collect(),
                ),
            );
            serialize_map(&map, out);
        }
        PluginValue::PhpClass(handle) => {
            let mut map: IndexMap<Vec<u8>, PluginValue> = IndexMap::new();
            map.insert(
                PHP_CLASS_KEY.to_vec(),
                PluginValue::string(handle.class.clone()),
            );
            serialize_map(&map, out);
        }
    }
}

fn serialize_bytes(bytes: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(b"s:");
    out.extend_from_slice(bytes.len().to_string().as_bytes());
    out.extend_from_slice(b":\"");
    out.extend_from_slice(bytes);
    out.extend_from_slice(b"\";");
}

fn serialize_map(map: &IndexMap<Vec<u8>, PluginValue>, out: &mut Vec<u8>) {
    out.extend_from_slice(b"a:");
    out.extend_from_slice(map.len().to_string().as_bytes());
    out.extend_from_slice(b":{");
    for (key, value) in map {
        match canonical_int_key(key) {
            Some(n) => {
                out.extend_from_slice(b"i:");
                out.extend_from_slice(n.to_string().as_bytes());
                out.push(b';');
            }
            None => serialize_bytes(key, out),
        }
        serialize_into(value, out);
    }
    out.push(b'}');
}

/// PHP canonicalizes array keys: a string key that is the canonical decimal form of an integer
/// (no leading zeros, no `-0`, within the platform int range) is stored as an int key, so such a
/// key can never appear as `s:...` on the wire.
fn canonical_int_key(key: &[u8]) -> Option<i64> {
    if key == b"0" {
        return Some(0);
    }
    let digits = key.strip_prefix(b"-").unwrap_or(key);
    match digits {
        [b'1'..=b'9', rest @ ..] if rest.iter().all(u8::is_ascii_digit) => {
            std::str::from_utf8(key).ok()?.parse().ok()
        }
        _ => None,
    }
}

/// Decodes a whole `serialize()` payload into a `PluginValue`, rejecting trailing garbage.
///
/// The decoder never produces `Object`: PHP's wire format erases the array/object distinction,
/// and object revival is banned anyway (`allowed_classes: false` on the PHP side). Arrays whose
/// keys are exactly `0..N` decode as `List`; anything else decodes as `Array`. Arrays carrying
/// the reserved handle-descriptor key sets decode as the corresponding handle.
pub fn unserialize(payload: &[u8]) -> anyhow::Result<PluginValue> {
    let mut pos = 0;
    let value = parse_value(payload, &mut pos)?;
    if pos != payload.len() {
        bail!("trailing garbage after serialized value at byte {pos}");
    }
    Ok(value)
}

/// One lexed step of a serialized payload: either a complete non-array value, or the opening of
/// an array whose entries follow.
enum Lex {
    Value(PluginValue),
    ArrayOpen(usize),
}

/// An in-progress array while parsing iteratively. The parser deliberately does not recurse:
/// nesting depth must never translate into call stack depth, so a hostile or corrupted payload
/// cannot overflow the stack (the explicit depth cap exists on top of that).
struct ArrayFrame {
    entries: IndexMap<Vec<u8>, PluginValue>,
    count: usize,
    parsed: usize,
    is_list: bool,
    pending_key: Option<Vec<u8>>,
}

fn parse_value(payload: &[u8], pos: &mut usize) -> anyhow::Result<PluginValue> {
    let mut stack: Vec<ArrayFrame> = Vec::new();
    let mut completed: Option<PluginValue> = None;

    loop {
        if let Some(value) = completed.take() {
            match stack.last_mut() {
                None => return Ok(value),
                Some(frame) => {
                    let key = frame
                        .pending_key
                        .take()
                        .expect("a completed value always follows a parsed key");
                    frame.entries.insert(key, value);
                    frame.parsed += 1;
                }
            }
        }

        if let Some(frame) = stack.last_mut()
            && frame.pending_key.is_none()
        {
            if frame.parsed == frame.count {
                if payload.get(*pos) != Some(&b'}') {
                    bail!("expected closing brace at byte {}", *pos);
                }
                *pos += 1;
                let frame = stack.pop().expect("frame was just observed");
                completed = Some(finish_array(frame)?);
                continue;
            }
            let index = frame.parsed as i64;
            match lex(payload, pos)? {
                Lex::Value(PluginValue::Int(n)) => {
                    frame.is_list &= n == index;
                    frame.pending_key = Some(n.to_string().into_bytes());
                }
                Lex::Value(PluginValue::String(bytes)) => {
                    frame.is_list = false;
                    frame.pending_key = Some(bytes);
                }
                Lex::Value(other) => bail!("array key is neither int nor string: {other:?}"),
                Lex::ArrayOpen(_) => bail!("array key is neither int nor string"),
            }
            continue;
        }

        match lex(payload, pos)? {
            Lex::Value(value) => completed = Some(value),
            Lex::ArrayOpen(count) => {
                if stack.len() >= MAX_DECODE_DEPTH {
                    bail!(
                        "serialized value exceeds the maximum nesting depth of {MAX_DECODE_DEPTH}"
                    );
                }
                stack.push(ArrayFrame {
                    entries: IndexMap::new(),
                    count,
                    parsed: 0,
                    is_list: true,
                    pending_key: None,
                });
            }
        }
    }
}

fn lex(payload: &[u8], pos: &mut usize) -> anyhow::Result<Lex> {
    let Some(tag) = payload.get(*pos..*pos + 2) else {
        bail!("truncated serialized value at byte {}", *pos);
    };
    *pos += 2;
    match tag {
        b"N;" => Ok(Lex::Value(PluginValue::Null)),
        b"b:" => match take_until(payload, pos, b';')? {
            b"0" => Ok(Lex::Value(PluginValue::Bool(false))),
            b"1" => Ok(Lex::Value(PluginValue::Bool(true))),
            other => bail!(
                "malformed bool payload: {:?}",
                String::from_utf8_lossy(other)
            ),
        },
        b"i:" => {
            let bytes = take_until(payload, pos, b';')?;
            let n = std::str::from_utf8(bytes).ok().and_then(|s| s.parse().ok());
            match n {
                Some(n) => Ok(Lex::Value(PluginValue::Int(n))),
                None => bail!(
                    "malformed int payload: {:?}",
                    String::from_utf8_lossy(bytes)
                ),
            }
        }
        b"d:" => {
            let bytes = take_until(payload, pos, b';')?;
            let f = std::str::from_utf8(bytes).ok().and_then(|s| match s {
                // Rust's float parser accepts these spellings too, but be explicit about the
                // exact special forms PHP emits.
                "INF" => Some(f64::INFINITY),
                "-INF" => Some(f64::NEG_INFINITY),
                "NAN" => Some(f64::NAN),
                _ => s.parse().ok(),
            });
            match f {
                Some(f) => Ok(Lex::Value(PluginValue::Float(f))),
                None => bail!(
                    "malformed float payload: {:?}",
                    String::from_utf8_lossy(bytes)
                ),
            }
        }
        b"s:" => Ok(Lex::Value(PluginValue::String(parse_string_body(
            payload, pos,
        )?))),
        b"a:" => {
            let count_bytes = take_until(payload, pos, b':')?;
            let count: usize = std::str::from_utf8(count_bytes)
                .ok()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "malformed array count: {:?}",
                        String::from_utf8_lossy(count_bytes)
                    )
                })?;
            if payload.get(*pos) != Some(&b'{') {
                bail!("expected opening brace at byte {}", *pos);
            }
            *pos += 1;
            Ok(Lex::ArrayOpen(count))
        }
        _ => bail!(
            "unknown serialized type tag {:?} at byte {}",
            String::from_utf8_lossy(tag),
            *pos - 2
        ),
    }
}

fn finish_array(frame: ArrayFrame) -> anyhow::Result<PluginValue> {
    if let Some(handle) = decode_handle(&frame.entries)? {
        return Ok(handle);
    }
    Ok(if frame.is_list {
        PluginValue::List(frame.entries.into_values().collect())
    } else {
        PluginValue::Array(frame.entries)
    })
}

fn parse_string_body(payload: &[u8], pos: &mut usize) -> anyhow::Result<Vec<u8>> {
    let len_bytes = take_until(payload, pos, b':')?;
    let len: usize = std::str::from_utf8(len_bytes)
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "malformed string length: {:?}",
                String::from_utf8_lossy(len_bytes)
            )
        })?;
    if payload.get(*pos) != Some(&b'"') {
        bail!("expected opening quote at byte {}", *pos);
    }
    *pos += 1;
    let Some(bytes) = payload.get(*pos..*pos + len) else {
        bail!("truncated string body at byte {}", *pos);
    };
    *pos += len;
    if payload.get(*pos..*pos + 2) != Some(b"\";") {
        bail!("expected closing quote at byte {}", *pos);
    }
    *pos += 2;
    Ok(bytes.to_vec())
}

/// Recognizes the reserved handle-descriptor arrays by their exact key sets.
fn decode_handle(entries: &IndexMap<Vec<u8>, PluginValue>) -> anyhow::Result<Option<PluginValue>> {
    let get = |key: &[u8]| entries.get(key);

    if entries.len() == 1 {
        if let Some(value) = get(PHP_CLASS_KEY) {
            let PluginValue::String(class) = value else {
                bail!("__pclass descriptor with a non-string class: {value:?}");
            };
            return Ok(Some(PluginValue::PhpClass(PhpClassHandle {
                class: descriptor_utf8(class, "__pclass")?,
            })));
        }
        return Ok(None);
    }

    if let Some(value) = get(RUST_HANDLE_KEY) {
        let allowed = entries
            .keys()
            .all(|k| k == RUST_HANDLE_KEY || k == CLASS_KEY || k == EPOCH_KEY || k == SNAPSHOT_KEY);
        if !allowed || entries.len() < 3 {
            bail!("malformed __rhandle descriptor: {entries:?}");
        }
        let rhandle = descriptor_u64(value, "__rhandle")?;
        let class = descriptor_class(get(CLASS_KEY), "__rhandle")?;
        let epoch = descriptor_u64(
            get(EPOCH_KEY).ok_or_else(|| anyhow::anyhow!("__rhandle descriptor lacks __epoch"))?,
            "__epoch",
        )?;
        let snapshot = match get(SNAPSHOT_KEY) {
            None => None,
            Some(PluginValue::Array(map)) => Some(map.clone()),
            Some(PluginValue::List(items)) => Some(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (i.to_string().into_bytes(), v.clone()))
                    .collect(),
            ),
            Some(other) => bail!("__snapshot is not an array: {other:?}"),
        };
        return Ok(Some(PluginValue::RustHandle(RustObjHandle {
            rhandle,
            class,
            epoch,
            snapshot,
        })));
    }

    if let Some(value) = get(PHP_HANDLE_KEY) {
        let allowed = entries
            .keys()
            .all(|k| k == PHP_HANDLE_KEY || k == CLASS_KEY || k == IMPLEMENTS_KEY);
        if !allowed || entries.len() != 3 {
            bail!("malformed __phandle descriptor: {entries:?}");
        }
        let phandle = descriptor_u64(value, "__phandle")?;
        let class = descriptor_class(get(CLASS_KEY), "__phandle")?;
        let implements = match get(IMPLEMENTS_KEY) {
            Some(PluginValue::List(items)) => items
                .iter()
                .map(|item| match item {
                    PluginValue::String(name) => descriptor_utf8(name, "__implements"),
                    other => bail!("__implements entry is not a string: {other:?}"),
                })
                .collect::<anyhow::Result<_>>()?,
            other => bail!("__implements is not a list: {other:?}"),
        };
        return Ok(Some(PluginValue::PhpHandle(PhpObjHandle {
            phandle,
            class,
            implements,
        })));
    }

    Ok(None)
}

fn descriptor_u64(value: &PluginValue, what: &str) -> anyhow::Result<u64> {
    match value {
        PluginValue::Int(n) => u64::try_from(*n)
            .map_err(|_| anyhow::anyhow!("{what} descriptor holds a negative id: {n}")),
        other => bail!("{what} descriptor id is not an int: {other:?}"),
    }
}

fn descriptor_class(value: Option<&PluginValue>, what: &str) -> anyhow::Result<String> {
    match value {
        Some(PluginValue::String(class)) => descriptor_utf8(class, what),
        other => bail!("{what} descriptor lacks a string __class: {other:?}"),
    }
}

fn descriptor_utf8(bytes: &[u8], what: &str) -> anyhow::Result<String> {
    String::from_utf8(bytes.to_vec())
        .map_err(|_| anyhow::anyhow!("{what} descriptor holds a non-UTF-8 class name"))
}

fn take_until<'a>(payload: &'a [u8], pos: &mut usize, terminator: u8) -> anyhow::Result<&'a [u8]> {
    let start = *pos;
    let Some(offset) = payload
        .get(start..)
        .and_then(|rest| rest.iter().position(|&b| b == terminator))
    else {
        bail!("unterminated field at byte {start}");
    };
    let bytes = &payload[start..start + offset];
    *pos = start + offset + 1;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(value: PluginValue) {
        let encoded = serialize(&value);
        let decoded = unserialize(&encoded).expect("decode failed");
        assert_eq!(
            decoded,
            value,
            "encoded form: {:?}",
            String::from_utf8_lossy(&encoded)
        );
    }

    #[test]
    fn encodes_scalars_like_php() {
        assert_eq!(serialize(&PluginValue::Null), b"N;");
        assert_eq!(serialize(&PluginValue::Bool(true)), b"b:1;");
        assert_eq!(serialize(&PluginValue::Bool(false)), b"b:0;");
        assert_eq!(serialize(&PluginValue::Int(-42)), b"i:-42;");
        assert_eq!(serialize(&PluginValue::Float(1.5)), b"d:1.5;");
        assert_eq!(serialize(&PluginValue::Float(2.0)), b"d:2;");
        assert_eq!(serialize(&PluginValue::Float(1e17)), b"d:1.0E+17;");
        assert_eq!(serialize(&PluginValue::string("ab")), b"s:2:\"ab\";");
        assert_eq!(
            serialize(&PluginValue::String(vec![0xff, 0x00, 0xfe])),
            b"s:3:\"\xff\x00\xfe\";"
        );
    }

    #[test]
    fn encodes_arrays_with_php_key_canonicalization() {
        let mut map: IndexMap<Vec<u8>, PluginValue> = IndexMap::new();
        map.insert(b"5".to_vec(), PluginValue::Int(1));
        map.insert(b"05".to_vec(), PluginValue::Int(2));
        map.insert(b"-0".to_vec(), PluginValue::Int(3));
        map.insert(b"x".to_vec(), PluginValue::Int(4));
        assert_eq!(
            serialize(&PluginValue::Array(map)),
            b"a:4:{i:5;i:1;s:2:\"05\";i:2;s:2:\"-0\";i:3;s:1:\"x\";i:4;}".as_slice(),
        );
    }

    #[test]
    fn object_is_encode_only_and_collapses_to_array() {
        let mut map: IndexMap<Vec<u8>, PluginValue> = IndexMap::new();
        map.insert(b"a".to_vec(), PluginValue::Int(1));
        let encoded = serialize(&PluginValue::Object(map.clone()));
        assert_eq!(encoded, serialize(&PluginValue::Array(map.clone())));
        assert_eq!(unserialize(&encoded).unwrap(), PluginValue::Array(map));
    }

    #[test]
    fn roundtrips_composites() {
        roundtrip(PluginValue::List(vec![
            PluginValue::Null,
            PluginValue::Bool(true),
            PluginValue::Int(7),
            PluginValue::Float(0.5),
            PluginValue::String(vec![0x80, 0x81]),
        ]));

        let mut inner: IndexMap<Vec<u8>, PluginValue> = IndexMap::new();
        inner.insert(vec![0xff, b'k'], PluginValue::string("v"));
        inner.insert(b"10".to_vec(), PluginValue::List(vec![]));
        roundtrip(PluginValue::Array(inner));
    }

    #[test]
    fn roundtrips_handles() {
        roundtrip(PluginValue::RustHandle(RustObjHandle {
            rhandle: 3,
            class: "Composer\\Script\\Event".to_string(),
            epoch: 1,
            snapshot: None,
        }));
        roundtrip(PluginValue::PhpHandle(PhpObjHandle {
            phandle: 8,
            class: "MyPlugin".to_string(),
            implements: vec!["Composer\\Plugin\\PluginInterface".to_string()],
        }));
        roundtrip(PluginValue::PhpClass(PhpClassHandle {
            class: "MyPlugin".to_string(),
        }));
    }

    #[test]
    fn decodes_sparse_int_keys_as_array_and_reencodes_identically() {
        let wire = b"a:2:{i:5;i:1;i:0;i:2;}".as_slice();
        let decoded = unserialize(wire).unwrap();
        let PluginValue::Array(ref map) = decoded else {
            panic!("expected an array, got {decoded:?}");
        };
        assert_eq!(map.get(b"5".as_slice()), Some(&PluginValue::Int(1)));
        assert_eq!(serialize(&decoded), wire);
    }

    #[test]
    fn rejects_over_deep_nesting() {
        let mut payload = Vec::new();
        for _ in 0..(MAX_DECODE_DEPTH + 2) {
            payload.extend_from_slice(b"a:1:{i:0;");
        }
        payload.extend_from_slice(b"N;");
        payload.extend(std::iter::repeat_n(b'}', MAX_DECODE_DEPTH + 2));
        let err = unserialize(&payload).unwrap_err();
        assert!(err.to_string().contains("nesting depth"), "{err}");
    }

    #[test]
    fn rejects_trailing_garbage_and_truncation() {
        assert!(unserialize(b"i:42;i:43;").is_err());
        assert!(unserialize(b"s:5:\"ab\";").is_err());
        assert!(unserialize(b"a:2:{i:0;i:1;}").is_err());
    }

    #[test]
    fn php_mixed_conversions() {
        let mixed = PhpMixed::Array(
            [
                ("a".to_string(), PhpMixed::Int(1)),
                ("b".to_string(), PhpMixed::List(vec![PhpMixed::Null])),
            ]
            .into_iter()
            .collect(),
        );
        let value = PluginValue::from_php_mixed(&mixed);
        assert_eq!(value.to_php_mixed().unwrap(), mixed);

        let handle = PluginValue::PhpClass(PhpClassHandle {
            class: "X".to_string(),
        });
        assert!(handle.to_php_mixed().is_err());
    }
}
