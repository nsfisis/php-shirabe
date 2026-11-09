//! ref: composer/vendor/composer/xdebug-handler/src/XdebugHandler.php sha1:b9341902dbfe674026faa2c362fc6513277ef605
//!
//! Keeping Xdebug out of the PHP worker, which is where Composer's restart of itself lands in
//! this port. See `docs/dev/xdebug.md`.

use bstr::{BStr, BString};
use shirabe_php_rpc::{PluginValue, WorkerConfig, query};
use shirabe_php_shim::{getenv, os_str_from_bytes, php_str};

/// `XdebugHandler::$name.XdebugHandler::SUFFIX_ALLOW`, where the name is the uppercased prefix of
/// the one construction Composer makes: `new XdebugHandler('Composer')` in `bin/composer`.
const ALLOW: &BStr = php_str(b"COMPOSER_ALLOW_XDEBUG");

/// PHP: `XdebugHandler::isXdebugActive()`. Whether Xdebug is loaded and running in an active mode.
///
/// Answered without asking PHP whenever the worker is started with the mode switched off, since
/// that settles the question for every Xdebug that honours the setting — Xdebug 2, which has no
/// such setting, is reported inactive while it is not. `diagnose` reports what the worker measures
/// instead.
pub fn is_xdebug_active() -> anyhow::Result<bool> {
    if switches_xdebug_off() {
        return Ok(false);
    }

    xdebug_active()
}

/// Whether the worker is started with Xdebug switched off, which is this port's stand-in for
/// `XdebugHandler::check()` restarting the process. The answer is a property of the environment
/// alone, so it holds whether or not the worker has been spawned yet.
fn switches_xdebug_off() -> bool {
    // PHP: `!((bool) explode('|', getenv($this->envAllowXdebug))[0])`, where the pipe-separated
    // form is the handoff to a process xdebug-handler restarted. Nothing restarts here, so what
    // is left is PHP's truthiness of the value.
    let allow_xdebug = getenv(os_str_from_bytes(ALLOW)).unwrap_or_default();
    allow_xdebug.is_empty() || allow_xdebug == std::ffi::OsStr::new("0")
}

/// The [`WorkerConfig`] the worker is spawned with: Xdebug switched off unless the user allows
/// it. The environment variable takes precedence over every ini setting, so switching the mode
/// off takes both. See `docs/dev/xdebug.md`.
pub(crate) fn worker_config() -> WorkerConfig {
    let mut config = WorkerConfig::default();
    if switches_xdebug_off() {
        config
            .ini_settings
            .push((BString::from("xdebug.mode"), BString::from("off")));
        config.env.push(("XDEBUG_MODE".into(), "off".into()));
    }
    config
}

/// What `Composer\XdebugHandler\XdebugHandler::getAllIniFiles()` measures: `[(string)
/// php_ini_loaded_file()]` merged with the trimmed, comma-split `php_ini_scanned_files()` list
/// when scanning is active. The worker runs on the ini files of the machine, so these are the
/// user's own.
pub fn get_all_ini_files() -> anyhow::Result<Vec<std::path::PathBuf>> {
    query("__shirabe_get_all_ini_files", PluginValue::Null)
}

/// `Composer\XdebugHandler\XdebugHandler::isXdebugActive()` as measured in the worker, which is
/// the process Xdebug is loaded into.
fn xdebug_active() -> anyhow::Result<bool> {
    query("__shirabe_xdebug_active", PluginValue::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_string_lists() {
        // XdebugHandler::getAllIniFiles() always yields at least one entry, which is the empty
        // string when no php.ini is loaded.
        assert!(!get_all_ini_files().unwrap().is_empty());
    }

    #[test]
    fn worker_starts_with_xdebug_switched_off() {
        // The `-d xdebug.mode=off` half is invisible from PHP while the extension is not loaded
        // (an unregistered ini entry is not readable), so only the environment half — the one
        // that overrides every ini setting — can be asserted here.
        let value = shirabe_php_rpc::__call_function_in_new_worker(
            &worker_config(),
            BStr::new("getenv"),
            vec![PluginValue::string("XDEBUG_MODE")],
        )
        .expect("failed to ask the worker for its Xdebug mode");
        assert_eq!(value, PluginValue::string("off"));
    }
}
