//! The Rust half of the plugin runtime split: a real PHP child process executes the plugin code
//! (see `docs/dev/php-worker.md`), and the entities it can call back into live here. This module
//! has no Composer counterpart.
//!
//! - `table`: the R table holding those entities, and the handles PHP stubs carry for them
//! - `dispatch`: the `CallRustMethod` dispatcher and the runtime service endpoint (handle 0)
//! - `entities`: the methods each proxied class answers
//! - `args`: argument decoding for those methods

pub(crate) mod args;
pub(crate) mod dispatch;
pub(crate) mod entities;
pub mod table;
