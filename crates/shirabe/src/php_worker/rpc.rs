//! The Rust half of the plugin runtime split: a real PHP child process executes the plugin code
//! (see `docs/dev/php-worker.md`), and the entities it can call back into live here. This module
//! has no Composer counterpart.
//!
//! - `table`: the Rust entity table holding those entities, and the ids PHP stubs carry for them
//! - `entity`: what an entity is, and the context a forwarded call is answered in
//! - `forwarding`: a marker type for each forwarding class, and the routing of constructor and
//!   static calls to them, generated
//! - `dispatch`: the dispatcher of the worker's requests, including the runtime services
//! - `entities`: the entity declarations: what each proxied class answers
//! - `args`: argument decoding for those methods
//! - `binding`: what the generated bindings build on: receivers and argument and answer conversion

pub(crate) mod args;
pub mod binding;
pub(crate) mod dispatch;
pub(crate) mod entities;
pub mod entity;
pub(crate) mod forwarding;
pub mod table;
