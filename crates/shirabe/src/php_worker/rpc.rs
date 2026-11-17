//! The Rust half of the plugin runtime split: a real PHP child process executes the plugin code
//! (see `docs/dev/php-worker.md`), and the entities it can call back into live here. This module
//! has no Composer counterpart.
//!
//! - `table`: the Rust entity table holding those entities, and the ids PHP stubs carry for them
//! - `entity`: what an entity is, and the context a forwarded call is answered in
//! - `forwarding`: the members each forwarding class routes to the Rust side, generated
//! - `dispatch`: the dispatcher of the worker's requests, including the runtime services
//! - `entities`: the methods each proxied class answers
//! - `args`: argument decoding for those methods

pub(crate) mod args;
pub(crate) mod dispatch;
pub(crate) mod entities;
pub mod entity;
pub(crate) mod forwarding;
pub mod table;
