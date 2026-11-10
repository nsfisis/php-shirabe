//! Rust-to-PHP RPC over a Unix domain socket. See `docs/dev/php-rpc.md`.

mod builtins;
pub mod codec;
mod env;
pub mod frame;
pub mod session;
mod throw;
mod transport;
pub mod value;
mod worker;

pub use builtins::{
    PhpVersion, cached, class_exists, get_php_binary, get_php_version, memory_get_peak_usage,
    memory_get_usage, phpversion, query, rar, stream_resolve_include_path,
};
pub use codec::{FromPluginValue, ToPluginValue};
pub use throw::PhpThrow;
pub use transport::{
    RefuseCallbacks, RustMethodDispatcher, alloc_rhandle, call_function, call_internal_function,
    call_php_method, call_static_method, new_object, set_release_rust_handle_hook,
};
pub use value::{PhpObjHandle, PhpObject, PluginValue, RustObjHandle};
pub use worker::{
    __call_function_in_new_worker, WorkerConfig, set_worker_config_provider, worker_is_running,
};
