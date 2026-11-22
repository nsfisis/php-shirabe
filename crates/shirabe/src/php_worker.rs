//! The Rust side of the PHP worker: the code that drives the PHP child process plugins and scripts
//! run in, and answers the calls it makes back (see `docs/dev/php-worker.md`). This module has no
//! Composer counterpart.

pub mod bundle;
mod by_value;
pub mod call;
pub(crate) mod callable;
pub mod codec;
pub(crate) mod console;
pub mod diagnostics;
pub mod error_handler;
pub(crate) mod installed_versions;
pub(crate) mod job;
pub mod platform_info;
pub mod proxy;
mod pushed_statics;
pub mod rpc;
pub mod runtime;
pub(crate) mod throwable;
pub mod xdebug;

use bstr::{BStr, BString};
use shirabe_php_rpc::WorkerConfig;
use shirabe_php_shim::php_str;

/// The worker-side classes of this crate, under `php/runtime/`.
const RUNTIME_FILES: &[(&BStr, &[u8])] = &[
    (
        php_str(b"Shirabe/ComposerEntryPoints.php"),
        include_bytes!("../php/runtime/Shirabe/ComposerEntryPoints.php"),
    ),
    (
        php_str(b"Shirabe/PlatformRuntime.php"),
        include_bytes!("../php/runtime/Shirabe/PlatformRuntime.php"),
    ),
];

/// Registers what the worker is spawned with (see [`worker_config`]) and how an exception it
/// throws is rebuilt (see [`throwable::throw_into_error`]). Called once at startup, before
/// anything can spawn the worker.
pub fn install() {
    shirabe_php_rpc::set_worker_config_provider(worker_config);
    shirabe_php_rpc::set_throw_reviver(throwable::throw_into_error);
}

/// The [`WorkerConfig`] the worker is spawned with: the entry points of
/// `Shirabe\ComposerEntryPoints` and the Xdebug settings of [`xdebug::add_worker_settings`].
pub(crate) fn worker_config() -> WorkerConfig {
    let mut config = WorkerConfig {
        runtime_files: RUNTIME_FILES,
        entry_point_classes: vec![BString::from(r"Shirabe\ComposerEntryPoints")],
        ..WorkerConfig::default()
    };
    xdebug::add_worker_settings(&mut config);
    config
}
