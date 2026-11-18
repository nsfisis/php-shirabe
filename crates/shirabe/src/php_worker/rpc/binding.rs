//! What the code `#[php_binding]` and `php_entity!` generate builds on: how an adapter reaches
//! the Rust value a forwarded call is answered by, and how arguments and answers cross the
//! boundary.
//!
//! An adapter decodes every argument first, then borrows the receiver for the call alone, and
//! converts the answer after the borrow ends: decoding an argument and registering an entity
//! the answer names may borrow other entities, or the receiver itself.

use crate::composer::ComposerWeakHandle;
use crate::event_dispatcher::EventFlags;
use crate::package::{
    AliasPackageHandle, DisplayMode, Link, Mirror, PackageHandle, PackageInterfaceHandle,
    RootPackageInterfaceHandle,
};
use crate::php_worker::job;
use crate::php_worker::job::JobOwner;
use crate::php_worker::rpc::entity::{Call, Entity, EntityArg, entity_value};
use crate::plugin::FileDownloadContext;
use crate::repository::{
    InstalledRepositoryInterfaceHandle, LockArrayRepositoryHandle, RepositoryInterfaceHandle,
};
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

/// A Rust return type encoded as the answer of a forwarded call. `failed` turns an error the
/// Rust side reports into the `Throw` the call crosses back as (`Call::failed`).
pub trait IntoRpcAnswer {
    fn into_rpc_answer(
        self,
        failed: &dyn Fn(anyhow::Error) -> PhpThrow,
    ) -> Result<RpcValue, PhpThrow>;
}

impl<H: Entity> IntoRpcAnswer for H {
    fn into_rpc_answer(self, _: &dyn Fn(anyhow::Error) -> PhpThrow) -> Result<RpcValue, PhpThrow> {
        entity_value(&self)
    }
}

macro_rules! to_rpc_value_answers {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoRpcAnswer for $ty {
                fn into_rpc_answer(self, _: &dyn Fn(anyhow::Error) -> PhpThrow) -> Result<RpcValue, PhpThrow> {
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
    fn into_rpc_answer(self, _: &dyn Fn(anyhow::Error) -> PhpThrow) -> Result<RpcValue, PhpThrow> {
        Ok(RpcValue::Null)
    }
}

impl<T: IntoRpcAnswer> IntoRpcAnswer for anyhow::Result<T> {
    fn into_rpc_answer(
        self,
        failed: &dyn Fn(anyhow::Error) -> PhpThrow,
    ) -> Result<RpcValue, PhpThrow> {
        self.map_err(failed)?.into_rpc_answer(failed)
    }
}

impl<T: IntoRpcAnswer> IntoRpcAnswer for Option<T> {
    fn into_rpc_answer(
        self,
        failed: &dyn Fn(anyhow::Error) -> PhpThrow,
    ) -> Result<RpcValue, PhpThrow> {
        match self {
            Some(value) => value.into_rpc_answer(failed),
            None => Ok(RpcValue::Null),
        }
    }
}

impl<T: IntoRpcAnswer> IntoRpcAnswer for Vec<T> {
    fn into_rpc_answer(
        self,
        failed: &dyn Fn(anyhow::Error) -> PhpThrow,
    ) -> Result<RpcValue, PhpThrow> {
        self.into_iter()
            .map(|item| item.into_rpc_answer(failed))
            .collect::<Result<_, _>>()
            .map(RpcValue::List)
    }
}

impl IntoRpcAnswer for &StreamContextOptions {
    fn into_rpc_answer(self, _: &dyn Fn(anyhow::Error) -> PhpThrow) -> Result<RpcValue, PhpThrow> {
        Ok(self.clone().to_rpc_value())
    }
}

/// A list the Rust side lends crosses as a copy.
impl<T: IntoRpcAnswer + Clone> IntoRpcAnswer for &Vec<T> {
    fn into_rpc_answer(
        self,
        failed: &dyn Fn(anyhow::Error) -> PhpThrow,
    ) -> Result<RpcValue, PhpThrow> {
        self.clone().into_rpc_answer(failed)
    }
}

/// An `array<string, T>` as PHP shapes it: an empty map crosses as a list, since an empty PHP
/// array is indistinguishable from an empty list.
impl<T: IntoRpcAnswer> IntoRpcAnswer for IndexMap<BString, T> {
    fn into_rpc_answer(
        self,
        failed: &dyn Fn(anyhow::Error) -> PhpThrow,
    ) -> Result<RpcValue, PhpThrow> {
        if self.is_empty() {
            return Ok(RpcValue::List(Vec::new()));
        }
        self.into_iter()
            .map(|(key, value)| Ok((key, value.into_rpc_answer(failed)?)))
            .collect::<Result<_, _>>()
            .map(RpcValue::Array)
    }
}

/// A real package crosses as a package entity.
impl IntoRpcAnswer for PackageHandle {
    fn into_rpc_answer(self, _: &dyn Fn(anyhow::Error) -> PhpThrow) -> Result<RpcValue, PhpThrow> {
        entity_value(&PackageInterfaceHandle::from(self))
    }
}

/// An alias package crosses as a package entity.
impl IntoRpcAnswer for AliasPackageHandle {
    fn into_rpc_answer(self, _: &dyn Fn(anyhow::Error) -> PhpThrow) -> Result<RpcValue, PhpThrow> {
        entity_value(&PackageInterfaceHandle::from(self))
    }
}

/// A `LockArrayRepository` crosses as a repository entity.
impl IntoRpcAnswer for LockArrayRepositoryHandle {
    fn into_rpc_answer(self, _: &dyn Fn(anyhow::Error) -> PhpThrow) -> Result<RpcValue, PhpThrow> {
        entity_value(&RepositoryInterfaceHandle::from(self))
    }
}

impl IntoRpcAnswer for &bstr::BStr {
    fn into_rpc_answer(self, _: &dyn Fn(anyhow::Error) -> PhpThrow) -> Result<RpcValue, PhpThrow> {
        Ok(self.to_rpc_value())
    }
}

impl IntoRpcAnswer for chrono::DateTime<chrono::Utc> {
    fn into_rpc_answer(self, _: &dyn Fn(anyhow::Error) -> PhpThrow) -> Result<RpcValue, PhpThrow> {
        Ok(self.to_rpc_value())
    }
}

/// A map the Rust side shares between readers crosses as a copy.
impl<T: IntoRpcAnswer + Clone> IntoRpcAnswer for std::rc::Rc<IndexMap<BString, T>> {
    fn into_rpc_answer(
        self,
        failed: &dyn Fn(anyhow::Error) -> PhpThrow,
    ) -> Result<RpcValue, PhpThrow> {
        std::rc::Rc::unwrap_or_clone(self).into_rpc_answer(failed)
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

/// What a job a forwarded call starts is registered with, besides its future: see
/// `job::register`.
pub(crate) struct JobContext {
    pub(crate) owner: JobOwner,
    pub(crate) owner_entity: std::rc::Rc<dyn std::any::Any>,
    pub(crate) canceled: PhpThrow,
}

impl std::fmt::Debug for JobContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobContext")
            .field("owner", &self.owner)
            .finish_non_exhaustive()
    }
}

/// A route whose `async fn` methods answer with a promise: the job each call starts.
pub(crate) trait JobReceiver {
    fn job(&self, call: &Call<'_>) -> Result<JobContext, PhpThrow>;
}

/// Registers the future an `async fn` adapter built as a job, and answers with its promise.
pub(crate) fn register_job(
    job: JobContext,
    future: impl FnOnce(
        std::rc::Rc<std::cell::Cell<bool>>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<RpcValue, PhpThrow>>>,
    >,
) -> RpcValue {
    let started = std::rc::Rc::new(std::cell::Cell::new(false));
    job::register(
        job.owner,
        job.owner_entity,
        started.clone(),
        job.canceled,
        future(started),
    )
}

/// Whether one handle forwards both an `async fn` and a `&mut self` method: the former holds its
/// borrow of the receiver across `.await`, where the RPC session can re-enter the latter.
pub const fn mixes_async_and_mut(kinds: &[(bool, bool)]) -> bool {
    let mut has_async = false;
    let mut has_mut = false;
    let mut index = 0;
    while index < kinds.len() {
        has_async |= kinds[index].0;
        has_mut |= kinds[index].1;
        index += 1;
    }
    has_async && has_mut
}

/// A shared entity the Rust side lends crosses as itself.
impl<T: ?Sized> IntoRpcAnswer for &std::rc::Rc<T>
where
    std::rc::Rc<T>: Entity,
{
    fn into_rpc_answer(self, _: &dyn Fn(anyhow::Error) -> PhpThrow) -> Result<RpcValue, PhpThrow> {
        entity_value(self)
    }
}

impl IntoRpcAnswer for &std::path::Path {
    fn into_rpc_answer(self, _: &dyn Fn(anyhow::Error) -> PhpThrow) -> Result<RpcValue, PhpThrow> {
        Ok(self.to_rpc_value())
    }
}

/// A map the Rust side lends crosses as a copy.
impl<T: IntoRpcAnswer + Clone> IntoRpcAnswer for &IndexMap<BString, T> {
    fn into_rpc_answer(
        self,
        failed: &dyn Fn(anyhow::Error) -> PhpThrow,
    ) -> Result<RpcValue, PhpThrow> {
        self.clone().into_rpc_answer(failed)
    }
}

/// An event's flags cross as the array PHP keeps them in.
impl IntoRpcAnswer for &EventFlags {
    fn into_rpc_answer(self, _: &dyn Fn(anyhow::Error) -> PhpThrow) -> Result<RpcValue, PhpThrow> {
        Ok(PhpMixed::Array(self.to_php_array()).to_rpc_value())
    }
}

/// The `$composer` of a script, package or installer event.
impl IntoRpcAnswer for &ComposerWeakHandle {
    fn into_rpc_answer(self, _: &dyn Fn(anyhow::Error) -> PhpThrow) -> Result<RpcValue, PhpThrow> {
        let composer = self
            .upgrade()
            .ok_or_else(|| PhpThrow::runtime("the Composer instance of this event is gone"))?;
        entity_value(&composer)
    }
}

/// An installed repository crosses as a repository entity.
impl IntoRpcAnswer for InstalledRepositoryInterfaceHandle {
    fn into_rpc_answer(self, _: &dyn Fn(anyhow::Error) -> PhpThrow) -> Result<RpcValue, PhpThrow> {
        entity_value(&self.as_repository_handle())
    }
}

/// The `$context` of a file download event as the PHP value the event stub returns.
impl IntoRpcAnswer for &FileDownloadContext {
    fn into_rpc_answer(self, _: &dyn Fn(anyhow::Error) -> PhpThrow) -> Result<RpcValue, PhpThrow> {
        match self {
            FileDownloadContext::Package(package) => entity_value(package),
            // TODO(plugin): the array a repository passes names the repository itself, which
            // cannot cross the boundary yet.
            FileDownloadContext::Repository => Err(PhpThrow::runtime(
                "the repository context of a file download event is not available over RPC yet",
            )),
        }
    }
}
