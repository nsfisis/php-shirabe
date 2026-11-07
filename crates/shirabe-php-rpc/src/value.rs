//! Plugin-boundary value model and its wire codec. The wire format is documented in
//! `docs/dev/php-rpc.md`.
//!
//! `PluginValue` is the value type crossing the Rust/PHP RPC boundary. It is deliberately
//! independent from `shirabe_php_shim::PhpMixed`: handles exist only at the plugin boundary, the
//! one conversion between the two is the `ToWire` / `FromWire` pair of `PhpMixed` in `wire.rs`,
//! and the codec below is a separate implementation from `shirabe_php_shim::var::serialize`.
//!
//! Strings and array keys are byte strings (`BString`), matching PHP's string semantics: the
//! codec must round-trip non-UTF-8 byte sequences losslessly.

use anyhow::bail;
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{ToPhpString, canonical_int_key, utf8_text};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, Weak};

/// A handle to an object whose entity lives on the Rust side. PHP holds a thin proxy stub.
#[derive(Debug, Clone, PartialEq)]
pub struct RustObjHandle {
    pub rhandle: u64,
    pub class: BString,
}

/// An owning handle to an object whose entity lives in the PHP child process.
///
/// The worker's P table keeps the entity alive while any clone of the handle lives. One phandle
/// has one shared entry on this side however many times its descriptor is decoded, and dropping
/// the last clone queues a `ReleasePhpHandle`, which goes out ahead of the next frame sent to the
/// worker. Handles compare equal exactly when they name the same entity.
#[derive(Debug, Clone)]
pub struct PhpObjHandle(Arc<PhpObjHandleEntry>);

#[derive(Debug)]
struct PhpObjHandleEntry {
    phandle: u64,
    class: BString,
    /// Every interface the entity's class implements (`class_implements` in the worker).
    implements: Vec<BString>,
}

#[derive(Debug, Default)]
struct PhpHandleTable {
    live: IndexMap<u64, Weak<PhpObjHandleEntry>>,
    released: Vec<u64>,
}

static PHP_HANDLES: LazyLock<Mutex<PhpHandleTable>> = LazyLock::new(Mutex::default);

fn php_handles() -> MutexGuard<'static, PhpHandleTable> {
    PHP_HANDLES
        .lock()
        .unwrap_or_else(|e| panic!("PHP RPC: P handle table mutex poisoned: {e}"))
}

impl PhpObjHandle {
    pub(crate) fn intern(phandle: u64, class: BString, implements: Vec<BString>) -> PhpObjHandle {
        let mut table = php_handles();
        if let Some(entry) = table.live.get(&phandle).and_then(Weak::upgrade) {
            return PhpObjHandle(entry);
        }
        // A release still queued for this phandle has not reached the worker, so its P-table
        // entry is intact and this handle takes it over.
        table.released.retain(|&released| released != phandle);
        let entry = Arc::new(PhpObjHandleEntry {
            phandle,
            class,
            implements,
        });
        table.live.insert(phandle, Arc::downgrade(&entry));
        PhpObjHandle(entry)
    }

    pub(crate) fn phandle(&self) -> u64 {
        self.0.phandle
    }

    pub fn class(&self) -> &BStr {
        self.0.class.as_bstr()
    }

    pub fn implements(&self) -> &[BString] {
        &self.0.implements
    }
}

impl PartialEq for PhpObjHandle {
    fn eq(&self, other: &PhpObjHandle) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for PhpObjHandle {}

impl std::hash::Hash for PhpObjHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.phandle.hash(state);
    }
}

impl Drop for PhpObjHandleEntry {
    fn drop(&mut self) {
        let mut table = php_handles();
        // The phandle may already name a newer entry, decoded after this one became unreachable.
        let current = table
            .live
            .get(&self.phandle)
            .is_some_and(|live| std::ptr::eq(live.as_ptr(), self));
        if current {
            table.live.swap_remove(&self.phandle);
            table.released.push(self.phandle);
        }
    }
}

/// Takes the phandles whose last handle has dropped since the previous call, for the
/// `ReleasePhpHandle` frames that go out ahead of the next frame.
pub(crate) fn take_released_php_handles() -> Vec<u64> {
    std::mem::take(&mut php_handles().released)
}

/// The `O:` record of a serialized PHP object: a class name and a property table, with the
/// property names carrying PHP's visibility mangling (hence the accessors below rather than
/// bare names).
#[derive(Debug, Clone, PartialEq)]
pub struct PhpObject {
    pub class: BString,
    pub props: IndexMap<BString, PluginValue>,
}

impl PhpObject {
    pub fn new(class: impl Into<BString>) -> PhpObject {
        PhpObject {
            class: class.into(),
            props: IndexMap::new(),
        }
    }

    /// A public property keeps its declared name.
    pub fn public(&self, name: impl AsRef<[u8]>) -> Option<&PluginValue> {
        self.props.get(name.as_ref())
    }

    pub fn set_public(&mut self, name: impl AsRef<[u8]>, value: PluginValue) {
        self.props.insert(name.as_ref().into(), value);
    }

    /// PHP mangles a protected property name to `\0*\0name`.
    pub fn protected(&self, name: impl AsRef<[u8]>) -> Option<&PluginValue> {
        self.props.get(&protected_key(name.as_ref()))
    }

    pub fn set_protected(&mut self, name: impl AsRef<[u8]>, value: PluginValue) {
        self.props.insert(protected_key(name.as_ref()), value);
    }

    /// PHP mangles a private property name to `\0<declaring class>\0name`. The declaring class is
    /// the one whose body holds the `private` declaration, which is not `self.class` once a
    /// subclass inherits it.
    pub fn private(
        &self,
        declaring_class: impl AsRef<[u8]>,
        name: impl AsRef<[u8]>,
    ) -> Option<&PluginValue> {
        self.props
            .get(&private_key(declaring_class.as_ref(), name.as_ref()))
    }

    pub fn set_private(
        &mut self,
        declaring_class: impl AsRef<[u8]>,
        name: impl AsRef<[u8]>,
        value: PluginValue,
    ) {
        self.props
            .insert(private_key(declaring_class.as_ref(), name.as_ref()), value);
    }
}

fn protected_key(name: &[u8]) -> BString {
    let mut key = BString::from(b"\0*\0");
    key.extend_from_slice(name);
    key
}

fn private_key(declaring_class: &[u8], name: &[u8]) -> BString {
    let mut key = BString::from(vec![0]);
    key.extend_from_slice(declaring_class);
    key.push(0);
    key.extend_from_slice(name);
    key
}

/// The value model of the plugin RPC boundary: PHP scalars, arrays, object records, and handle
/// descriptors. An array decodes as `List` when its keys are exactly `0..N` and as `Array`
/// otherwise. A descriptor crosses as the object record of a class reserved for it (see
/// `RUST_HANDLE_CLASS` and its siblings), so no array is ever read as one.
#[derive(Debug, Clone, PartialEq)]
pub enum PluginValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(BString),
    List(Vec<PluginValue>),
    Array(IndexMap<BString, PluginValue>),
    PhpObject(PhpObject),
    RustHandle(RustObjHandle),
    PhpHandle(PhpObjHandle),
    /// A job in the registry both sides hold an entry for (see `docs/dev/plugin-async.md`). The
    /// Rust side allocates odd ids, the worker even ones.
    Job(u64),
}

impl PluginValue {
    pub fn string(s: impl Into<BString>) -> PluginValue {
        PluginValue::String(s.into())
    }
}

/// Maximum nesting depth the decoder accepts before rejecting the payload, so a corrupted or
/// hostile payload cannot overflow the stack.
pub const MAX_DECODE_DEPTH: usize = 512;

/// The descriptor classes, declared in the worker's `php/runtime/Shirabe/Wire/`. The worker never
/// writes an object of its own as one of these records (it crosses as a `PhpHandle` like any other
/// object), so a record of one of these classes is always a descriptor.
const RUST_HANDLE_CLASS: &[u8] = b"Shirabe\\Wire\\RustHandle";
const PHP_HANDLE_CLASS: &[u8] = b"Shirabe\\Wire\\PhpHandle";
const JOB_CLASS: &[u8] = b"Shirabe\\Wire\\Job";

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
            out.extend_from_slice(&n.to_php_string());
            out.push(b';');
        }
        PluginValue::Float(f) => {
            out.extend_from_slice(b"d:");
            let mut repr = BString::default();
            shirabe_php_src::zend::zend_smart_str::smart_str_append_double(
                &mut repr, *f, -1, false,
            );
            out.extend_from_slice(&repr);
            out.push(b';');
        }
        PluginValue::String(bytes) => serialize_bytes(bytes, out),
        PluginValue::List(items) => {
            out.extend_from_slice(b"a:");
            out.extend_from_slice(&items.len().to_php_string());
            out.extend_from_slice(b":{");
            for (index, item) in items.iter().enumerate() {
                out.extend_from_slice(b"i:");
                out.extend_from_slice(&index.to_php_string());
                out.push(b';');
                serialize_into(item, out);
            }
            out.push(b'}');
        }
        PluginValue::Array(map) => serialize_map(map, out),
        PluginValue::PhpObject(object) => serialize_object(object, out),
        PluginValue::RustHandle(handle) => {
            let mut record = PhpObject::new(RUST_HANDLE_CLASS);
            record.set_public(
                "rhandle",
                PluginValue::Int(i64::try_from(handle.rhandle).expect("rhandle exceeds i64")),
            );
            record.set_public("class", PluginValue::string(handle.class.clone()));
            serialize_object(&record, out);
        }
        PluginValue::PhpHandle(handle) => {
            let mut record = PhpObject::new(PHP_HANDLE_CLASS);
            record.set_public(
                "phandle",
                PluginValue::Int(i64::try_from(handle.phandle()).expect("phandle exceeds i64")),
            );
            record.set_public("class", PluginValue::string(handle.class()));
            record.set_public(
                "implements",
                PluginValue::List(
                    handle
                        .implements()
                        .iter()
                        .map(|name| PluginValue::string(name.clone()))
                        .collect(),
                ),
            );
            serialize_object(&record, out);
        }
        PluginValue::Job(id) => {
            let mut record = PhpObject::new(JOB_CLASS);
            record.set_public(
                "id",
                PluginValue::Int(i64::try_from(*id).expect("job id exceeds i64")),
            );
            serialize_object(&record, out);
        }
    }
}

fn serialize_object(object: &PhpObject, out: &mut Vec<u8>) {
    out.extend_from_slice(b"O:");
    out.extend_from_slice(&object.class.len().to_php_string());
    out.extend_from_slice(b":\"");
    out.extend_from_slice(&object.class);
    out.extend_from_slice(b"\":");
    out.extend_from_slice(&object.props.len().to_php_string());
    out.extend_from_slice(b":{");
    for (name, value) in &object.props {
        // A property name is always written as a string, even a numeric one, and never as the
        // int key an array of the same shape would get.
        serialize_bytes(name, out);
        serialize_into(value, out);
    }
    out.push(b'}');
}

fn serialize_bytes(bytes: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(b"s:");
    out.extend_from_slice(&bytes.len().to_php_string());
    out.extend_from_slice(b":\"");
    out.extend_from_slice(bytes);
    out.extend_from_slice(b"\";");
}

fn serialize_map(map: &IndexMap<BString, PluginValue>, out: &mut Vec<u8>) {
    out.extend_from_slice(b"a:");
    out.extend_from_slice(&map.len().to_php_string());
    out.extend_from_slice(b":{");
    for (key, value) in map {
        match canonical_int_key(key) {
            Some(n) => {
                out.extend_from_slice(b"i:");
                out.extend_from_slice(&n.to_php_string());
                out.push(b';');
            }
            None => serialize_bytes(key, out),
        }
        serialize_into(value, out);
    }
    out.push(b'}');
}

/// Decodes a whole `serialize()` payload into a `PluginValue`, rejecting trailing garbage.
///
/// Arrays whose keys are exactly `0..N` decode as `List`; anything else decodes as `Array`. An
/// `O:` record of a descriptor class decodes as the handle or job it describes, and any other
/// `O:` record as `PhpObject`.
pub fn unserialize(payload: &[u8]) -> anyhow::Result<PluginValue> {
    let mut pos = 0;
    let value = parse_value(payload, &mut pos)?;
    if pos != payload.len() {
        bail!("trailing garbage after serialized value at byte {pos}");
    }
    Ok(value)
}

/// One lexed step of a serialized payload: a complete scalar, the opening of a container whose
/// entries follow, or a back-reference to an earlier value.
enum Lex {
    Value(PluginValue),
    /// The entry count, and the class name of an object record.
    ContainerOpen(usize, Option<BString>),
    BackReference(usize),
}

/// An in-progress array or object record while parsing iteratively. The parser deliberately does
/// not recurse: nesting depth must never translate into call stack depth, so a hostile or
/// corrupted payload cannot overflow the stack (the explicit depth cap exists on top of that).
struct Frame {
    entries: IndexMap<BString, PluginValue>,
    /// The class name of an object record; `None` for an array.
    class: Option<BString>,
    /// This container's own number in the payload's value numbering.
    number: usize,
    count: usize,
    parsed: usize,
    is_list: bool,
    pending_key: Option<BString>,
}

#[expect(
    clippy::disallowed_macros,
    reason = "an anyhow error about a malformed payload, raised by the port"
)]
fn parse_value(payload: &[u8], pos: &mut usize) -> anyhow::Result<PluginValue> {
    let mut stack: Vec<Frame> = Vec::new();
    let mut completed: Option<PluginValue> = None;
    // PHP numbers every value of a payload from 1 in document order — never a key, but including
    // a back-reference itself — and an `r:` record names one of those numbers. Only an object can
    // be named that way (an array crosses by copy), so only objects are kept for resolution;
    // holding every value would cost a slot per scalar for payloads that never reference one.
    let mut numbered_objects: IndexMap<usize, PluginValue> = IndexMap::new();
    let mut values = 0;

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
                let number = frame.number;
                let is_object = frame.class.is_some();
                let value = finish_frame(frame)?;
                if is_object {
                    numbered_objects.insert(number, value.clone());
                }
                completed = Some(value);
                continue;
            }
            let index = frame.parsed as i64;
            match lex(payload, pos)? {
                Lex::Value(PluginValue::Int(n)) => {
                    frame.is_list &= n == index;
                    frame.pending_key = Some(n.to_php_string());
                }
                Lex::Value(PluginValue::String(bytes)) => {
                    frame.is_list = false;
                    frame.pending_key = Some(bytes);
                }
                Lex::Value(other) => bail!("array key is neither int nor string: {other:?}"),
                Lex::ContainerOpen(..) | Lex::BackReference(_) => {
                    bail!("array key is neither int nor string")
                }
            }
            continue;
        }

        values += 1;
        match lex(payload, pos)? {
            Lex::Value(value) => completed = Some(value),
            Lex::BackReference(number) => match numbered_objects.get(&number) {
                // An immutable value has no identity to preserve on this side, so a repeated
                // instance decodes as a copy of the one it names.
                Some(value) => completed = Some(value.clone()),
                None => bail!(
                    "back-reference r:{number} at byte {} does not name a completed object; a \
                     cyclic object graph cannot cross the plugin boundary",
                    *pos
                ),
            },
            Lex::ContainerOpen(count, class) => {
                if stack.len() >= MAX_DECODE_DEPTH {
                    bail!(
                        "serialized value exceeds the maximum nesting depth of {MAX_DECODE_DEPTH}"
                    );
                }
                stack.push(Frame {
                    entries: IndexMap::new(),
                    class,
                    number: values,
                    count,
                    parsed: 0,
                    is_list: true,
                    pending_key: None,
                });
            }
        }
    }
}

#[expect(
    clippy::disallowed_macros,
    reason = "an anyhow error about a malformed payload, raised by the port"
)]
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
            other => bail!("malformed bool payload: {:?}", BStr::new(other)),
        },
        b"i:" => {
            let bytes = take_until(payload, pos, b';')?;
            let n = utf8_text(bytes).and_then(|s| s.parse().ok());
            match n {
                Some(n) => Ok(Lex::Value(PluginValue::Int(n))),
                None => bail!("malformed int payload: {:?}", BStr::new(bytes)),
            }
        }
        b"d:" => {
            let bytes = take_until(payload, pos, b';')?;
            let f = utf8_text(bytes).and_then(|s| match s {
                // Rust's float parser accepts these spellings too, but be explicit about the
                // exact special forms PHP emits.
                "INF" => Some(f64::INFINITY),
                "-INF" => Some(f64::NEG_INFINITY),
                "NAN" => Some(f64::NAN),
                _ => s.parse().ok(),
            });
            match f {
                Some(f) => Ok(Lex::Value(PluginValue::Float(f))),
                None => bail!("malformed float payload: {:?}", BStr::new(bytes)),
            }
        }
        b"s:" => Ok(Lex::Value(PluginValue::String(parse_string_body(
            payload, pos,
        )?))),
        b"a:" => Ok(Lex::ContainerOpen(parse_entry_count(payload, pos)?, None)),
        b"O:" => {
            let class = parse_quoted(payload, pos, b':')?;
            Ok(Lex::ContainerOpen(
                parse_entry_count(payload, pos)?,
                Some(class),
            ))
        }
        b"r:" => {
            let bytes = take_until(payload, pos, b';')?;
            match utf8_text(bytes).and_then(|s| s.parse().ok()) {
                Some(number) => Ok(Lex::BackReference(number)),
                None => bail!("malformed back-reference: {:?}", BStr::new(bytes)),
            }
        }
        // A PHP reference aliases a variable; nothing on this side can carry that aliasing, and
        // no value the boundary produces is written by reference.
        b"R:" => bail!(
            "a PHP reference (`R:`) at byte {} cannot cross the plugin boundary",
            *pos - 2
        ),
        _ => bail!(
            "unknown serialized type tag {:?} at byte {}",
            BStr::new(tag),
            *pos - 2
        ),
    }
}

fn finish_frame(frame: Frame) -> anyhow::Result<PluginValue> {
    if let Some(class) = frame.class {
        return decode_object(class, frame.entries);
    }
    Ok(if frame.is_list {
        PluginValue::List(frame.entries.into_values().collect())
    } else {
        PluginValue::Array(frame.entries)
    })
}

/// The `<count>:{` a container's entries follow.
#[expect(
    clippy::disallowed_macros,
    reason = "an anyhow error about a malformed payload, raised by the port"
)]
fn parse_entry_count(payload: &[u8], pos: &mut usize) -> anyhow::Result<usize> {
    let count_bytes = take_until(payload, pos, b':')?;
    let count: usize = utf8_text(count_bytes)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("malformed entry count: {:?}", BStr::new(count_bytes)))?;
    if payload.get(*pos) != Some(&b'{') {
        bail!("expected opening brace at byte {}", *pos);
    }
    *pos += 1;
    Ok(count)
}

fn parse_string_body(payload: &[u8], pos: &mut usize) -> anyhow::Result<BString> {
    parse_quoted(payload, pos, b';')
}

/// A length-prefixed quoted run: `<len>:"<bytes>"` followed by `terminator`.
#[expect(
    clippy::disallowed_macros,
    reason = "an anyhow error about a malformed payload, raised by the port"
)]
fn parse_quoted(payload: &[u8], pos: &mut usize, terminator: u8) -> anyhow::Result<BString> {
    let len_bytes = take_until(payload, pos, b':')?;
    let len: usize = utf8_text(len_bytes)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("malformed string length: {:?}", BStr::new(len_bytes)))?;
    if payload.get(*pos) != Some(&b'"') {
        bail!("expected opening quote at byte {}", *pos);
    }
    *pos += 1;
    let Some(bytes) = payload.get(*pos..*pos + len) else {
        bail!("truncated string body at byte {}", *pos);
    };
    *pos += len;
    if payload.get(*pos) != Some(&b'"') || payload.get(*pos + 1) != Some(&terminator) {
        bail!("expected closing quote at byte {}", *pos);
    }
    *pos += 2;
    Ok(bytes.into())
}

/// Decodes an `O:` record: a descriptor class into the handle or job it describes, and any other
/// class into a `PhpObject`.
fn decode_object(
    class: BString,
    props: IndexMap<BString, PluginValue>,
) -> anyhow::Result<PluginValue> {
    let expect_props = |names: &[&[u8]]| -> anyhow::Result<()> {
        if props.len() != names.len() || !names.iter().all(|name| props.contains_key(*name)) {
            bail!("malformed {class:?} record: {props:?}");
        }
        Ok(())
    };
    match class.as_slice() {
        RUST_HANDLE_CLASS => {
            expect_props(&[b"rhandle", b"class"])?;
            Ok(PluginValue::RustHandle(RustObjHandle {
                rhandle: descriptor_id(&props[b"rhandle".as_slice()], &class)?,
                class: descriptor_class(&props[b"class".as_slice()], &class)?,
            }))
        }
        PHP_HANDLE_CLASS => {
            expect_props(&[b"phandle", b"class", b"implements"])?;
            let implements = match &props[b"implements".as_slice()] {
                PluginValue::List(items) => items
                    .iter()
                    .map(|item| match item {
                        PluginValue::String(name) => Ok(name.clone()),
                        other => bail!("{class:?} implements a non-string: {other:?}"),
                    })
                    .collect::<anyhow::Result<_>>()?,
                other => bail!("{class:?} implements is not a list: {other:?}"),
            };
            Ok(PluginValue::PhpHandle(PhpObjHandle::intern(
                descriptor_id(&props[b"phandle".as_slice()], &class)?,
                descriptor_class(&props[b"class".as_slice()], &class)?,
                implements,
            )))
        }
        JOB_CLASS => {
            expect_props(&[b"id"])?;
            Ok(PluginValue::Job(descriptor_id(
                &props[b"id".as_slice()],
                &class,
            )?))
        }
        _ => Ok(PluginValue::PhpObject(PhpObject { class, props })),
    }
}

fn descriptor_id(value: &PluginValue, record: &BString) -> anyhow::Result<u64> {
    match value {
        PluginValue::Int(n) => u64::try_from(*n)
            .map_err(|_| anyhow::anyhow!("{record:?} record holds a negative id: {n}")),
        other => bail!("{record:?} record id is not an int: {other:?}"),
    }
}

fn descriptor_class(value: &PluginValue, record: &BString) -> anyhow::Result<BString> {
    match value {
        PluginValue::String(class) => Ok(class.clone()),
        other => bail!("{record:?} record class is not a string: {other:?}"),
    }
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
        assert_eq!(decoded, value, "encoded form: {:?}", BStr::new(&encoded));
    }

    /// Handles compare by identity, and decoding a phandle that is still held yields the held
    /// entry, so the handle is dropped before decoding to make the decoder read the descriptor.
    /// Each test uses its own phandle: the handle table is shared by every test thread.
    fn roundtrip_php_handle(phandle: u64, class: &[u8], implements: &[&[u8]]) {
        let implements: Vec<BString> = implements.iter().map(|&name| name.into()).collect();
        let encoded = serialize(&PluginValue::PhpHandle(PhpObjHandle::intern(
            phandle,
            class.into(),
            implements.clone(),
        )));
        let decoded = unserialize(&encoded).expect("decode failed");
        let PluginValue::PhpHandle(ref handle) = decoded else {
            panic!("expected a PHP handle, got {decoded:?}");
        };
        assert_eq!(handle.phandle(), phandle);
        assert_eq!(handle.class(), class);
        assert_eq!(handle.implements(), implements.as_slice());
        assert_eq!(serialize(&decoded), encoded);
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
            serialize(&PluginValue::String(vec![0xff, 0x00, 0xfe].into())),
            b"s:3:\"\xff\x00\xfe\";"
        );
    }

    #[test]
    fn encodes_arrays_with_php_key_canonicalization() {
        let mut map: IndexMap<BString, PluginValue> = IndexMap::new();
        map.insert(b"5".into(), PluginValue::Int(1));
        map.insert(b"05".into(), PluginValue::Int(2));
        map.insert(b"-0".into(), PluginValue::Int(3));
        map.insert(b"x".into(), PluginValue::Int(4));
        assert_eq!(
            serialize(&PluginValue::Array(map)),
            b"a:4:{i:5;i:1;s:2:\"05\";i:2;s:2:\"-0\";i:3;s:1:\"x\";i:4;}".as_slice(),
        );
    }

    /// The bytes are what PHP writes for
    /// `new MatchAllConstraint()` and `new DateTimeImmutable('2026-08-07 12:34:56.123456', new
    /// DateTimeZone('UTC'))`: a protected property is mangled, a public one is not, and both are
    /// written as string keys.
    #[test]
    fn encodes_object_records_like_php() {
        let mut constraint = PhpObject::new("Composer\\Semver\\Constraint\\MatchAllConstraint");
        constraint.set_protected("prettyString", PluginValue::Null);
        assert_eq!(
            serialize(&PluginValue::PhpObject(constraint)),
            b"O:45:\"Composer\\Semver\\Constraint\\MatchAllConstraint\":1:{s:15:\"\0*\0prettyString\";N;}".as_slice(),
        );

        // A private property carries the declaring class rather than `*`, which is what lets a
        // subclass hold its own property of the same name.
        let mut response = PhpObject::new("Composer\\Util\\Http\\Response");
        response.set_private(
            "Composer\\Util\\Http\\Response",
            "code",
            PluginValue::Int(200),
        );
        assert_eq!(
            serialize(&PluginValue::PhpObject(response)),
            b"O:27:\"Composer\\Util\\Http\\Response\":1:{s:33:\"\0Composer\\Util\\Http\\Response\0code\";i:200;}".as_slice(),
        );

        let mut date = PhpObject::new("DateTimeImmutable");
        date.set_public("date", PluginValue::string("2026-08-07 12:34:56.123456"));
        date.set_public("timezone_type", PluginValue::Int(3));
        date.set_public("timezone", PluginValue::string("UTC"));
        assert_eq!(
            serialize(&PluginValue::PhpObject(date)),
            b"O:17:\"DateTimeImmutable\":3:{s:4:\"date\";s:26:\"2026-08-07 12:34:56.123456\";s:13:\"timezone_type\";i:3;s:8:\"timezone\";s:3:\"UTC\";}".as_slice(),
        );
    }

    #[test]
    fn roundtrips_object_records() {
        let mut inner = PhpObject::new("Composer\\Semver\\Constraint\\Constraint");
        inner.set_protected("operator", PluginValue::Int(4));
        inner.set_protected("version", PluginValue::string("1.0.0"));
        let mut outer = PhpObject::new("Composer\\Package\\Link");
        outer.set_protected("source", PluginValue::string("a/b"));
        outer.set_protected("constraint", PluginValue::PhpObject(inner.clone()));
        roundtrip(PluginValue::PhpObject(outer.clone()));

        // Visibility is part of the property name: neither accessor sees the other's key.
        assert_eq!(inner.protected("operator"), Some(&PluginValue::Int(4)));
        assert_eq!(inner.public("operator"), None);
        assert_eq!(outer.protected("prettyConstraint"), None);

        let mut response = PhpObject::new("Composer\\Util\\Http\\Response");
        response.set_private(
            "Composer\\Util\\Http\\Response",
            "body",
            PluginValue::string("{}"),
        );
        roundtrip(PluginValue::PhpObject(response.clone()));
        assert_eq!(
            response.private("Composer\\Util\\Http\\Response", "body"),
            Some(&PluginValue::string("{}"))
        );
        assert_eq!(response.protected("body"), None);
        assert_eq!(response.public("body"), None);
    }

    /// PHP numbers every value of a payload, including the ones inside an object and the
    /// back-references themselves, so resolving `r:` means counting exactly the same way. Both
    /// payloads below are PHP's own output for `[$c, $c]` and `[$c, $c, $e, $e]`.
    #[test]
    fn resolves_back_references_by_phps_value_numbering() {
        let decoded =
            unserialize(b"a:2:{i:0;O:8:\"stdClass\":1:{s:1:\"p\";i:1;}i:1;r:2;}").unwrap();
        let PluginValue::List(items) = decoded else {
            panic!("expected a list, got {decoded:?}");
        };
        assert_eq!(items[0], items[1]);

        let decoded = unserialize(
            b"a:4:{i:0;O:8:\"stdClass\":1:{s:1:\"p\";i:1;}i:1;r:2;i:2;O:8:\"stdClass\":1:{s:1:\"q\";i:2;}i:3;r:5;}",
        )
        .unwrap();
        let PluginValue::List(items) = decoded else {
            panic!("expected a list, got {decoded:?}");
        };
        assert_eq!(items[0], items[1]);
        assert_eq!(items[2], items[3]);
        assert_ne!(items[0], items[2]);
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "an anyhow error raised by the port, read through core::fmt"
    )]
    #[test]
    fn rejects_cyclic_object_graphs_and_php_references() {
        let err = unserialize(b"O:8:\"stdClass\":1:{s:4:\"self\";r:1;}").unwrap_err();
        assert!(err.to_string().contains("cyclic"), "{err}");

        let err = unserialize(b"a:2:{i:0;i:1;i:1;R:2;}").unwrap_err();
        assert!(err.to_string().contains("PHP reference"), "{err}");

        let err = unserialize(b"a:1:{i:0;r:9;}").unwrap_err();
        assert!(err.to_string().contains("r:9"), "{err}");
    }

    #[test]
    fn roundtrips_composites() {
        roundtrip(PluginValue::List(vec![
            PluginValue::Null,
            PluginValue::Bool(true),
            PluginValue::Int(7),
            PluginValue::Float(0.5),
            PluginValue::String(vec![0x80, 0x81].into()),
        ]));

        let mut inner: IndexMap<BString, PluginValue> = IndexMap::new();
        inner.insert(vec![0xff, b'k'].into(), PluginValue::string("v"));
        inner.insert(b"10".into(), PluginValue::List(vec![]));
        roundtrip(PluginValue::Array(inner));
    }

    #[test]
    fn roundtrips_non_utf8_class_names() {
        let mut object = PhpObject::new(b"Caf\xe9");
        object.set_private(b"Caf\xe9", "p", PluginValue::Int(1));
        roundtrip(PluginValue::PhpObject(object));
        roundtrip(PluginValue::RustHandle(RustObjHandle {
            rhandle: 3,
            class: b"Caf\xe9".into(),
        }));
        roundtrip_php_handle(8, b"Caf\xe9", &[b"Plugin\xff"]);
    }

    #[test]
    fn roundtrips_handles() {
        roundtrip(PluginValue::RustHandle(RustObjHandle {
            rhandle: 3,
            class: "Composer\\Script\\Event".into(),
        }));
        roundtrip_php_handle(9, b"MyPlugin", &[b"Composer\\Plugin\\PluginInterface"]);
        roundtrip(PluginValue::Job(7));
    }

    #[test]
    fn decodes_arrays_shaped_like_descriptors_as_data() {
        let mut map = IndexMap::new();
        map.insert(BString::from("__rhandle"), PluginValue::Int(1));
        map.insert(
            BString::from("__class"),
            PluginValue::string("Composer\\Script\\Event"),
        );
        roundtrip(PluginValue::Array(map));
        let mut map = IndexMap::new();
        map.insert(BString::from("__job"), PluginValue::Int(7));
        roundtrip(PluginValue::Array(map));
    }

    #[test]
    fn rejects_malformed_descriptor_records() {
        assert!(unserialize(b"O:16:\"Shirabe\\Wire\\Job\":0:{}").is_err());
        assert!(
            unserialize(b"O:16:\"Shirabe\\Wire\\Job\":2:{s:2:\"id\";i:7;s:1:\"x\";N;}").is_err()
        );
        assert!(unserialize(b"O:16:\"Shirabe\\Wire\\Job\":1:{s:2:\"id\";s:1:\"7\";}").is_err());
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

    #[expect(
        clippy::disallowed_methods,
        reason = "an anyhow error raised by the port, read through core::fmt"
    )]
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
}
