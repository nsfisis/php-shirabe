mod array;
mod compress;
mod curl;
mod datetime;
mod env;
mod exception;
mod filter;
mod fs;
mod hash;
mod json;
mod math;
mod net;
mod openssl;
mod output;
mod phar;
mod preg;
mod process;
mod random;
mod rar;
mod runtime;
mod stream;
mod string;
mod url;
mod var;
mod xml;
mod zip;

pub use array::*;
pub use compress::*;
pub use curl::*;
pub use datetime::*;
pub use env::*;
pub use exception::*;
pub use filter::*;
pub use fs::*;
pub use hash::*;
pub use json::*;
pub use math::*;
pub use net::*;
pub use openssl::*;
pub use output::*;
pub use phar::*;
pub use preg::*;
pub use process::*;
pub use random::*;
pub use rar::*;
pub use runtime::*;
pub use stream::*;
pub use string::*;
pub use url::*;
pub use var::*;
pub use xml::*;
pub use zip::*;

use indexmap::IndexMap;

#[derive(Debug, Clone, Default)]
pub enum PhpMixed {
    #[default]
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<PhpMixed>),
    Array(IndexMap<String, PhpMixed>),
    // TODO: consolidate Object to Array.
    Object(IndexMap<String, PhpMixed>),
}

impl serde::Serialize for PhpMixed {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self {
            PhpMixed::Null => serializer.serialize_none(),
            PhpMixed::Bool(b) => serializer.serialize_bool(*b),
            PhpMixed::Int(i) => serializer.serialize_i64(*i),
            PhpMixed::Float(f) => serializer.serialize_f64(*f),
            PhpMixed::String(s) => serializer.serialize_str(s),
            PhpMixed::List(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            PhpMixed::Array(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (k, v) in entries {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
            PhpMixed::Object(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (k, v) in entries {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

/// PHP `===` semantics: type-strict and, for arrays, order-sensitive.
impl PartialEq for PhpMixed {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (PhpMixed::Null, PhpMixed::Null) => true,
            (PhpMixed::Bool(a), PhpMixed::Bool(b)) => a == b,
            (PhpMixed::Int(a), PhpMixed::Int(b)) => a == b,
            (PhpMixed::Float(a), PhpMixed::Float(b)) => a == b,
            (PhpMixed::String(a), PhpMixed::String(b)) => a == b,
            (PhpMixed::List(a), PhpMixed::List(b)) => a == b,
            (PhpMixed::Array(a), PhpMixed::Array(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|((ka, va), (kb, vb))| ka == kb && va == vb)
            }
            (PhpMixed::Object(a), PhpMixed::Object(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|((ka, va), (kb, vb))| ka == kb && va == vb)
            }
            _ => false,
        }
    }
}

impl PhpMixed {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            PhpMixed::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            PhpMixed::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            PhpMixed::Float(f) => Some(*f),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            PhpMixed::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&Vec<PhpMixed>> {
        match self {
            PhpMixed::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&IndexMap<String, PhpMixed>> {
        match self {
            PhpMixed::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut IndexMap<String, PhpMixed>> {
        match self {
            PhpMixed::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_list_mut(&mut self) -> Option<&mut Vec<PhpMixed>> {
        match self {
            PhpMixed::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&IndexMap<String, PhpMixed>> {
        match self {
            PhpMixed::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, PhpMixed::Null)
    }

    /// PHP loose boolean cast `(bool) $value`.
    pub fn to_bool(&self) -> bool {
        php_truthy(self)
    }

    pub fn get(&self, key: &str) -> Option<&PhpMixed> {
        self.as_array().and_then(|m| m.get(key))
    }

    /// Treats PhpMixed::Null as None, everything else as Some.
    pub fn as_opt(&self) -> Option<&PhpMixed> {
        if self.is_null() { None } else { Some(self) }
    }

    pub fn unwrap_or(self, default: PhpMixed) -> PhpMixed {
        if self.is_null() { default } else { self }
    }

    pub fn unwrap_or_default(self) -> PhpMixed {
        if self.is_null() { PhpMixed::Null } else { self }
    }

    pub fn unwrap(self) -> PhpMixed {
        if self.is_null() {
            panic!("called `PhpMixed::unwrap()` on a `Null` value");
        }
        self
    }

    /// Treats PhpMixed::Null as None and applies the function for chaining.
    pub fn and_then<U, F: FnOnce(&PhpMixed) -> Option<U>>(&self, f: F) -> Option<U> {
        self.as_opt().and_then(f)
    }

    /// Treats `Null` and `Bool(false)` as the falsy case, anything else as Some.
    pub fn ok_or_else<E, F: FnOnce() -> E>(self, err: F) -> Result<PhpMixed, E> {
        match self {
            PhpMixed::Null | PhpMixed::Bool(false) => Err(err()),
            v => Ok(v),
        }
    }
}

impl From<()> for PhpMixed {
    fn from(_value: ()) -> Self {
        PhpMixed::Null
    }
}

impl From<bool> for PhpMixed {
    fn from(value: bool) -> Self {
        PhpMixed::Bool(value)
    }
}

/// Blanket downcast helper so trait objects (`dyn Command`, `dyn OutputInterface`,
/// etc.) can be downcast to their concrete type, mirroring PHP `instanceof`.
pub trait AsAny {
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

impl<T: std::any::Any> AsAny for T {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl From<i64> for PhpMixed {
    fn from(value: i64) -> Self {
        PhpMixed::Int(value)
    }
}

impl From<f64> for PhpMixed {
    fn from(value: f64) -> Self {
        PhpMixed::Float(value)
    }
}

impl From<String> for PhpMixed {
    fn from(value: String) -> Self {
        PhpMixed::String(value)
    }
}

impl From<&str> for PhpMixed {
    fn from(value: &str) -> Self {
        PhpMixed::String(value.to_string())
    }
}

impl<T> From<IndexMap<String, T>> for PhpMixed
where
    T: Into<PhpMixed>,
{
    fn from(value: IndexMap<String, T>) -> Self {
        PhpMixed::Array(value.into_iter().map(|(k, v)| (k, v.into())).collect())
    }
}

impl<T> From<Vec<T>> for PhpMixed
where
    T: Into<PhpMixed>,
{
    fn from(value: Vec<T>) -> Self {
        PhpMixed::List(value.into_iter().map(|v| v.into()).collect())
    }
}

impl<T> From<Option<T>> for PhpMixed
where
    T: Into<PhpMixed>,
{
    fn from(value: Option<T>) -> Self {
        match value {
            Some(v) => v.into(),
            None => PhpMixed::Null,
        }
    }
}

impl<T> From<Box<T>> for PhpMixed
where
    T: Into<PhpMixed>,
{
    fn from(value: Box<T>) -> Self {
        (*value).into()
    }
}

impl std::fmt::Display for PhpMixed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.pad(&php_to_string(self))
    }
}

#[derive(Debug, Clone)]
pub enum PhpResource {
    Stdin,
    Stdout,
    Stderr,
    File(std::rc::Rc<std::cell::RefCell<std::fs::File>>),
}
