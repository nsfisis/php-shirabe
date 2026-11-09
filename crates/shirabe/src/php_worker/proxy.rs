//! Rust adapters for entities living in the PHP child process: a plugin, an installer, a
//! capability, a command. Each holds the entity's P-table handle and forwards the Rust trait it
//! implements over RPC. This module has no Composer counterpart.

mod capability;
mod command;
mod installer;
mod plugin;

pub use capability::*;
pub use command::*;
pub use installer::*;
pub use plugin::*;
