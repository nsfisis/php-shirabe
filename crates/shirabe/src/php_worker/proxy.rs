//! Rust adapters for entities living in the PHP child process: a plugin, an installer, a
//! capability, a command, a `RemoteFilesystem`. Each holds the entity's P-table handle and
//! forwards the Rust trait or methods it implements over RPC.

mod capability;
mod command;
mod installer;
mod plugin;
pub mod remote_filesystem;

pub use capability::*;
pub use command::*;
pub use installer::*;
pub use plugin::*;
