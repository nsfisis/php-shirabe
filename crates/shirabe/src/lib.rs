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

// InstalledVersions is intentionally unported to Rust. It is a runtime API for plugins and project
// code, never read by Composer itself. Its real state is stored in PHP's InstalledVersions class.
// See also `__shirabe_installed_versions_reload` in crates/shirabe-php-rpc/php/worker.php.

/// ref: composer/bin/composer
pub fn run(argv: Vec<String>) -> anyhow::Result<i32> {
    use crate::console::ApplicationHandle;
    use crate::util::Platform;
    use shirabe_symfony_console::input::ArgvInput;

    // TODO(php-runtime): the full initialization process in composer/bin/composer should be ported
    // somewhere else that communicates with the real PHP runtime.
    Platform::put_env(
        "COMPOSER_BINARY",
        &shirabe_php_shim::realpath(argv.first().map(String::as_str).unwrap_or_default())
            .unwrap_or_default(),
    );

    let application = ApplicationHandle::new("Composer".to_string(), String::new())?;
    let input = std::rc::Rc::new(std::cell::RefCell::new(ArgvInput::new(Some(argv), None)?));
    application.run(Some(input), None)
}
