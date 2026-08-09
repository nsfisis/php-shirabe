//! ref: composer/vendor/composer/xdebug-handler/src/XdebugHandler.php
//!
//! Keeping Xdebug out of the PHP worker, which is where Composer's restart of itself lands in
//! this port. See `docs/dev/xdebug.md`.

use shirabe_php_shim::getenv;

/// `XdebugHandler::$name.XdebugHandler::SUFFIX_ALLOW`, where the name is the uppercased prefix of
/// the one construction Composer makes: `new XdebugHandler('Composer')` in `bin/composer`.
const ALLOW: &str = "COMPOSER_ALLOW_XDEBUG";

/// PHP: `XdebugHandler::isXdebugActive()`. Whether Xdebug is loaded and running in an active mode.
///
/// Answered without asking PHP whenever the worker is started with the mode switched off, since
/// that settles the question for every Xdebug that honours the setting — Xdebug 2, which has no
/// such setting, is reported inactive while it is not. `diagnose` reports what the worker measures
/// instead.
pub fn is_xdebug_active() -> bool {
    if switches_xdebug_off() {
        return false;
    }

    crate::xdebug_active()
}

/// Whether the worker is started with Xdebug switched off, which is this port's stand-in for
/// `XdebugHandler::check()` restarting the process. The answer is a property of the environment
/// alone, so it holds whether or not the worker has been spawned yet.
pub(crate) fn switches_xdebug_off() -> bool {
    // PHP: `!((bool) explode('|', getenv($this->envAllowXdebug))[0])`, where the pipe-separated
    // form is the handoff to a process xdebug-handler restarted. Nothing restarts here, so what
    // is left is PHP's truthiness of the value.
    let allow_xdebug = getenv(ALLOW).unwrap_or_default();
    matches!(allow_xdebug.to_string_lossy().as_ref(), "" | "0")
}
