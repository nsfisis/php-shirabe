//! The Rust side of the PHP worker: the code that drives the PHP child process plugins and scripts
//! run in, and answers the calls it makes back (see `docs/dev/php-worker.md`). This module has no
//! Composer counterpart.

pub mod call;
pub mod codec;
pub(crate) mod console;
pub(crate) mod job;
pub mod proxy;
pub mod rpc;
pub(crate) mod runtime;
pub(crate) mod throwable;
