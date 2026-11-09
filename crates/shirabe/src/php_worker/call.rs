//! The calls the Rust side makes into the PHP worker.
//!
//! This module has no Composer counterpart: it is the Rust-to-PHP half of the plugin runtime
//! split (see `docs/dev/php-worker.md`). Every call serves the `CallRustMethod` requests the
//! worker makes while it is in flight with a `PluginRpcDispatcher` unless the caller brings a
//! dispatcher of its own, and decodes the answer as the type the caller asks for.

use crate::php_worker::rpc::dispatch::PluginRpcDispatcher;
use crate::php_worker::throwable::throw_into_error;
use bstr::BStr;
use shirabe_php_rpc::{FromWire, PhpObjHandle, PhpThrow, PluginValue, RustMethodDispatcher};
use shirabe_php_shim::{RuntimeException, bformat};

/// What a call into the worker runs.
#[derive(Debug, Clone, Copy)]
pub enum PhpCall<'a> {
    /// `$name(...$args)`, or the worker's entry point of that name.
    Function(&'a BStr),
    /// `$object->$method(...$args)` on a P-table entity.
    Method(&'a PhpObjHandle, &'a BStr),
    /// `$class::$method(...$args)`.
    StaticMethod(&'a BStr, &'a BStr),
    /// `new $class(...$args)`, answering with the P-table handle of the new entity.
    New(&'a BStr),
}

impl PhpCall<'_> {
    /// Runs the call. A PHP exception becomes the error of the ported exception it stands for.
    pub fn run<R: FromWire>(self, args: Vec<PluginValue>) -> anyhow::Result<R> {
        self.run_with(args, &mut PluginRpcDispatcher)
    }

    /// [`PhpCall::run`], for a caller that catches the PHP exception itself.
    pub fn try_run<R: FromWire>(
        self,
        args: Vec<PluginValue>,
    ) -> anyhow::Result<Result<R, PhpThrow>> {
        self.try_run_with(args, &mut PluginRpcDispatcher)
    }

    /// [`PhpCall::run`] with `dispatcher` serving the callbacks.
    pub fn run_with<R: FromWire>(
        self,
        args: Vec<PluginValue>,
        dispatcher: &mut dyn RustMethodDispatcher,
    ) -> anyhow::Result<R> {
        self.try_run_with(args, dispatcher)?
            .map_err(throw_into_error)
    }

    /// [`PhpCall::try_run`] with `dispatcher` serving the callbacks. An answer that is not an `R`
    /// is the error [`PhpCall::unsupported_shape`] describes.
    pub fn try_run_with<R: FromWire>(
        self,
        args: Vec<PluginValue>,
        dispatcher: &mut dyn RustMethodDispatcher,
    ) -> anyhow::Result<Result<R, PhpThrow>> {
        let outcome = match self {
            PhpCall::Function(name) => shirabe_php_rpc::call_function(name, args, dispatcher),
            PhpCall::Method(handle, method) => {
                shirabe_php_rpc::call_php_method(handle, method, args, dispatcher)
            }
            PhpCall::StaticMethod(class, method) => {
                shirabe_php_rpc::call_static_method(class, method, args, dispatcher)
            }
            PhpCall::New(class) => shirabe_php_rpc::new_object(class, args, dispatcher),
        }?;
        Ok(match outcome {
            Ok(value) => Ok(R::from_wire(&value).map_err(|throw| self.unsupported_shape(throw))?),
            Err(throw) => Err(throw),
        })
    }

    /// The error of an answer that did not decode, naming the call.
    pub fn unsupported_shape(self, throw: PhpThrow) -> anyhow::Error {
        let callee = match self {
            PhpCall::Function(name) => bformat!("{name}()"),
            PhpCall::Method(handle, method) => bformat!("{}::{method}()", handle.class()),
            PhpCall::StaticMethod(class, method) => bformat!("{class}::{method}()"),
            PhpCall::New(class) => bformat!("new {class}"),
        };
        RuntimeException::new(bformat!(
            "{callee} returned an unsupported shape over RPC: {}",
            throw.message
        ))
        .into()
    }
}
