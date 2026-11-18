//! What the code `#[php_binding]` and `php_entity!` generate builds on: how an adapter reaches
//! the Rust value a forwarded call is answered by, and how arguments and answers cross the
//! boundary.
//!
//! An adapter decodes every argument first, then borrows the receiver for the call alone, and
//! converts the answer after the borrow ends: decoding an argument and registering an entity
//! the answer names may borrow other entities, or the receiver itself.

use crate::package::{
    AliasPackageHandle, DisplayMode, Link, Mirror, PackageHandle, PackageInterfaceHandle,
    RootPackageInterfaceHandle,
};
use crate::php_worker::rpc::entity::{Call, Entity, EntityArg, entity_value};
use crate::repository::{LockArrayRepositoryHandle, RepositoryInterfaceHandle};
use crate::util::StreamContextOptions;
use crate::util::http::Response;
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_rpc::codec::DecodeError;
use shirabe_php_rpc::{FromRpcValue, PhpThrow, RpcValue, ToRpcValue, codec};
use shirabe_php_shim::PhpMixed;

/// A route from an entity to the Rust value answering its `&self` methods.
pub trait Receiver<T: ?Sized> {
    type Guard<'a>: std::ops::Deref<Target = T>
    where
        Self: 'a;

    fn get(&self) -> Self::Guard<'_>;
}

/// A route from an entity to the Rust value answering its `&mut self` methods.
pub trait ReceiverMut<T: ?Sized> {
    type Guard<'a>: std::ops::DerefMut<Target = T>
    where
        Self: 'a;

    fn get_mut(&self) -> Self::Guard<'_>;
}

/// A Rust argument type decoded from the PHP value a forwarded call passes.
pub trait FromRpcArg: Sized {
    fn from_rpc_arg(value: &RpcValue) -> Result<Self, DecodeError>;
}

/// Decodes the argument at `position`. A missing argument decodes the same way PHP's `null`
/// does.
pub fn decode_arg<T: FromRpcArg>(call: &Call<'_>, position: usize) -> Result<T, PhpThrow> {
    T::from_rpc_arg(call.args().get(position).unwrap_or(&codec::NULL))
        .map_err(|error| call.arg_throw(position, error))
}

impl<H: Entity> FromRpcArg for H {
    fn from_rpc_arg(value: &RpcValue) -> Result<Self, DecodeError> {
        EntityArg::<H>::from_rpc_value(value).map(|entity| entity.0)
    }
}

macro_rules! from_rpc_value_args {
    ($($ty:ty),* $(,)?) => {
        $(
            impl FromRpcArg for $ty {
                fn from_rpc_arg(value: &RpcValue) -> Result<Self, DecodeError> {
                    <$ty as FromRpcValue>::from_rpc_value(value)
                }
            }
        )*
    };
}

from_rpc_value_args!(
    bool,
    i64,
    f64,
    BString,
    std::path::PathBuf,
    PhpMixed,
    RpcValue,
    Link,
    Mirror,
    DisplayMode,
    chrono::DateTime<chrono::Utc>,
    StreamContextOptions,
);

impl<T: FromRpcArg> FromRpcArg for Option<T> {
    fn from_rpc_arg(value: &RpcValue) -> Result<Self, DecodeError> {
        match value {
            RpcValue::Null => Ok(None),
            value => T::from_rpc_arg(value).map(Some),
        }
    }
}

/// A PHP array as a list, accepting the keyed shape PHP allows anywhere a list is documented.
impl<T: FromRpcArg> FromRpcArg for Vec<T> {
    fn from_rpc_arg(value: &RpcValue) -> Result<Self, DecodeError> {
        match value {
            RpcValue::List(items) => items.iter().map(T::from_rpc_arg).collect(),
            RpcValue::Array(map) => map.values().map(T::from_rpc_arg).collect(),
            other => Err(codec::unexpected("an array", other)),
        }
    }
}

/// A PHP array as a map, decoded the way `FromRpcValue` decodes one.
impl<T: FromRpcArg> FromRpcArg for IndexMap<BString, T> {
    fn from_rpc_arg(value: &RpcValue) -> Result<Self, DecodeError> {
        IndexMap::<BString, RpcValue>::from_rpc_value(value)?
            .into_iter()
            .map(|(key, item)| Ok((key, T::from_rpc_arg(&item)?)))
            .collect()
    }
}

impl FromRpcArg for RootPackageInterfaceHandle {
    fn from_rpc_arg(value: &RpcValue) -> Result<Self, DecodeError> {
        PackageInterfaceHandle::from_rpc_arg(value)?
            .as_root()
            .ok_or_else(|| codec::unexpected("a RootPackageInterface", value))
    }
}

impl FromRpcArg for AliasPackageHandle {
    fn from_rpc_arg(value: &RpcValue) -> Result<Self, DecodeError> {
        PackageInterfaceHandle::from_rpc_arg(value)?
            .as_alias()
            .ok_or_else(|| codec::unexpected("an AliasPackage", value))
    }
}

/// A Rust return type encoded as the answer of a forwarded call.
pub trait IntoRpcAnswer {
    fn into_rpc_answer(self, call: &Call<'_>) -> Result<RpcValue, PhpThrow>;
}

impl<H: Entity> IntoRpcAnswer for H {
    fn into_rpc_answer(self, _: &Call<'_>) -> Result<RpcValue, PhpThrow> {
        entity_value(&self)
    }
}

macro_rules! to_rpc_value_answers {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoRpcAnswer for $ty {
                fn into_rpc_answer(self, _: &Call<'_>) -> Result<RpcValue, PhpThrow> {
                    Ok(self.to_rpc_value())
                }
            }
        )*
    };
}

to_rpc_value_answers!(
    bool,
    i64,
    f64,
    BString,
    std::path::PathBuf,
    PhpMixed,
    RpcValue,
    Link,
    Mirror,
    StreamContextOptions,
    Response,
);

impl IntoRpcAnswer for () {
    fn into_rpc_answer(self, _: &Call<'_>) -> Result<RpcValue, PhpThrow> {
        Ok(RpcValue::Null)
    }
}

impl<T: IntoRpcAnswer> IntoRpcAnswer for anyhow::Result<T> {
    fn into_rpc_answer(self, call: &Call<'_>) -> Result<RpcValue, PhpThrow> {
        self.map_err(call.failed())?.into_rpc_answer(call)
    }
}

impl<T: IntoRpcAnswer> IntoRpcAnswer for Option<T> {
    fn into_rpc_answer(self, call: &Call<'_>) -> Result<RpcValue, PhpThrow> {
        match self {
            Some(value) => value.into_rpc_answer(call),
            None => Ok(RpcValue::Null),
        }
    }
}

impl<T: IntoRpcAnswer> IntoRpcAnswer for Vec<T> {
    fn into_rpc_answer(self, call: &Call<'_>) -> Result<RpcValue, PhpThrow> {
        self.into_iter()
            .map(|item| item.into_rpc_answer(call))
            .collect::<Result<_, _>>()
            .map(RpcValue::List)
    }
}

impl IntoRpcAnswer for &StreamContextOptions {
    fn into_rpc_answer(self, _: &Call<'_>) -> Result<RpcValue, PhpThrow> {
        Ok(self.clone().to_rpc_value())
    }
}

/// A list the Rust side lends crosses as a copy.
impl<T: IntoRpcAnswer + Clone> IntoRpcAnswer for &Vec<T> {
    fn into_rpc_answer(self, call: &Call<'_>) -> Result<RpcValue, PhpThrow> {
        self.clone().into_rpc_answer(call)
    }
}

/// An `array<string, T>` as PHP shapes it: an empty map crosses as a list, since an empty PHP
/// array is indistinguishable from an empty list.
impl<T: IntoRpcAnswer> IntoRpcAnswer for IndexMap<BString, T> {
    fn into_rpc_answer(self, call: &Call<'_>) -> Result<RpcValue, PhpThrow> {
        if self.is_empty() {
            return Ok(RpcValue::List(Vec::new()));
        }
        self.into_iter()
            .map(|(key, value)| Ok((key, value.into_rpc_answer(call)?)))
            .collect::<Result<_, _>>()
            .map(RpcValue::Array)
    }
}

/// A real package crosses as a package entity.
impl IntoRpcAnswer for PackageHandle {
    fn into_rpc_answer(self, _: &Call<'_>) -> Result<RpcValue, PhpThrow> {
        entity_value(&PackageInterfaceHandle::from(self))
    }
}

/// An alias package crosses as a package entity.
impl IntoRpcAnswer for AliasPackageHandle {
    fn into_rpc_answer(self, _: &Call<'_>) -> Result<RpcValue, PhpThrow> {
        entity_value(&PackageInterfaceHandle::from(self))
    }
}

/// A `LockArrayRepository` crosses as a repository entity.
impl IntoRpcAnswer for LockArrayRepositoryHandle {
    fn into_rpc_answer(self, _: &Call<'_>) -> Result<RpcValue, PhpThrow> {
        entity_value(&RepositoryInterfaceHandle::from(self))
    }
}

impl IntoRpcAnswer for &bstr::BStr {
    fn into_rpc_answer(self, _: &Call<'_>) -> Result<RpcValue, PhpThrow> {
        Ok(self.to_rpc_value())
    }
}

impl IntoRpcAnswer for chrono::DateTime<chrono::Utc> {
    fn into_rpc_answer(self, _: &Call<'_>) -> Result<RpcValue, PhpThrow> {
        Ok(self.to_rpc_value())
    }
}

/// A map the Rust side shares between readers crosses as a copy.
impl<T: IntoRpcAnswer + Clone> IntoRpcAnswer for std::rc::Rc<IndexMap<BString, T>> {
    fn into_rpc_answer(self, call: &Call<'_>) -> Result<RpcValue, PhpThrow> {
        std::rc::Rc::unwrap_or_clone(self).into_rpc_answer(call)
    }
}

/// Whether the stub of a class declares the parameters an adapter decodes its arguments against
/// (`checked`, `None` for a hand-written adapter, which decodes whatever arrives): both are lists
/// of the PHP types `php-decls/` spells.
pub const fn params_match(checked: Option<&[&[u8]]>, declared: &[&[u8]]) -> bool {
    let Some(checked) = checked else {
        return true;
    };
    if checked.len() != declared.len() {
        return false;
    }
    let mut index = 0;
    while index < checked.len() {
        if !bytes_eq(checked[index], declared[index]) {
            return false;
        }
        index += 1;
    }
    true
}

const fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut index = 0;
    while index < a.len() {
        if a[index] != b[index] {
            return false;
        }
        index += 1;
    }
    true
}
