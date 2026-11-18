//! What the code `#[php_binding]` and `php_entity!` generate builds on: how an adapter reaches
//! the Rust value a forwarded call is answered by, and how arguments and answers cross the
//! boundary.
//!
//! An adapter decodes every argument first, then borrows the receiver for the call alone, and
//! converts the answer after the borrow ends: decoding an argument and registering an entity
//! the answer names may borrow other entities, or the receiver itself.

use crate::package::{
    DisplayMode, Link, Mirror, PackageInterfaceHandle, RootPackageInterfaceHandle,
};
use crate::php_worker::rpc::entity::{Call, Entity, EntityArg, entity_value};
use crate::repository::{LockArrayRepositoryHandle, RepositoryInterfaceHandle};
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

/// A `LockArrayRepository` crosses as a repository entity.
impl IntoRpcAnswer for LockArrayRepositoryHandle {
    fn into_rpc_answer(self, _: &Call<'_>) -> Result<RpcValue, PhpThrow> {
        entity_value(&RepositoryInterfaceHandle::from(self))
    }
}
