//! `Composer\Platform\Runtime` has no Rust counterpart. Its work belongs to the running PHP
//! interpreter, so it is ported as PHP into the RPC worker (`ShirabePlatformRuntime` in
//! `shirabe-php-rpc`), and its callers read the answers off `shirabe_php_rpc::PlatformInfo`.

pub mod hhvm_detector;
pub mod version;

pub use hhvm_detector::*;
pub use version::*;
