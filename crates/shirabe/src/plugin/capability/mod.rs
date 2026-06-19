#[allow(clippy::module_inception, reason = "to port PHP's structure as it is")]
pub mod capability;
pub mod command_provider;

pub use capability::*;
pub use command_provider::*;
