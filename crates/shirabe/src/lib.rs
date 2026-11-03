#![deny(clippy::disallowed_types, clippy::disallowed_macros)]

use bstr::BString;
use shirabe_php_shim::{path_from_bytes, path_to_bytes, realpath};
pub mod advisory;
pub mod autoload;
pub mod cache;
pub mod command;
pub mod composer;
pub mod config;
pub mod console;
pub mod dependency_resolver;
pub mod downloader;
pub mod event_dispatcher;
pub mod exception;
pub mod factory;
pub mod filter;
pub mod installer;
pub mod io;
pub mod json;
pub mod package;
pub mod phpstan;
pub mod platform;
pub mod plugin;
pub mod question;
pub mod repository;
pub mod script;
pub mod self_update;
pub mod signal;
pub mod util;

// Declared here to use this allocator in all binaries including tests and benches.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// InstalledVersions is intentionally unported to Rust. It is a runtime API for plugins and project
// code, never read by Composer itself. Its real state is stored in PHP's InstalledVersions class.
// See also `__shirabe_installed_versions_reload` in crates/shirabe-php-rpc/php/worker.php.

/// ref: composer/bin/composer
pub async fn run(argv: Vec<bstr::BString>) -> anyhow::Result<i32> {
    use crate::console::ApplicationHandle;
    use crate::util::Platform;
    use shirabe_symfony_console::input::ArgvInput;

    // TODO(php-runtime): the full initialization process in composer/bin/composer should be ported
    // somewhere else that communicates with the real PHP runtime.
    Platform::put_env(
        "COMPOSER_BINARY",
        realpath(path_from_bytes(
            argv.first().cloned().unwrap_or_default().as_slice(),
        ))
        .map(|p| bstr::BString::from(path_to_bytes(&p)))
        .unwrap_or_default(),
    );

    let application = ApplicationHandle::new(BString::from("Shirabe"), BString::default())?;
    let input = std::rc::Rc::new(std::cell::RefCell::new(ArgvInput::new(Some(argv), None)?));
    application.run(Some(input), None).await
}
