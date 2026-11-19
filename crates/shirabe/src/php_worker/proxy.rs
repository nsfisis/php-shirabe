//! Rust adapters for entities living in the PHP child process: a plugin, an installer, a
//! capability, a command, a `RemoteFilesystem`. Each holds the entity's PHP entity handle and
//! forwards the Rust trait or methods it implements over RPC.

mod capability;
mod command;
mod installer;
mod plugin;
pub mod remote_filesystem;

/// An adapter for an entity living in the PHP child process, whose trait impl
/// `#[php_binding(.., proxy = ..)]` generates: the entity the calls go to.
pub(crate) trait PhpProxy {
    fn php_handle(&self) -> &shirabe_php_rpc::PhpEntityHandle;
}

pub use capability::*;
pub use command::*;
pub use installer::*;
pub use plugin::*;
