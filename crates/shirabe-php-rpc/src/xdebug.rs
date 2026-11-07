//! ref: composer/vendor/composer/xdebug-handler/src/XdebugHandler.php sha1:b9341902dbfe674026faa2c362fc6513277ef605
//!
//! Keeping Xdebug out of the PHP worker, which is where Composer's restart of itself lands in
//! this port. See `docs/dev/xdebug.md`.

use crate::xdebug_active;
use bstr::BStr;
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
pub(crate) fn switches_xdebug_off() -> bool {
    // PHP: `!((bool) explode('|', getenv($this->envAllowXdebug))[0])`, where the pipe-separated
    // form is the handoff to a process xdebug-handler restarted. Nothing restarts here, so what
    // is left is PHP's truthiness of the value.
    let allow_xdebug = getenv(os_str_from_bytes(ALLOW)).unwrap_or_default();
    allow_xdebug.is_empty() || allow_xdebug == std::ffi::OsStr::new("0")
}
