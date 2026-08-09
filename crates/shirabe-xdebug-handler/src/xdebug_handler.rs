//! ref: composer/vendor/composer/xdebug-handler/src/XdebugHandler.php

#[derive(Debug)]
pub struct XdebugHandler;

impl XdebugHandler {
    pub fn is_xdebug_active() -> bool {
        // TODO(php-runtime)
        false
    }

    pub fn get_skipped_version() -> Option<String> {
        // TODO(php-runtime)
        // The restart-to-disable-xdebug mechanism is not ported (`is_xdebug_active` is
        // hardcoded `false`), so a restart never happens and `self::$skipped` stays at
        // its PHP default of `""`.
        Some(String::new())
    }

    pub fn get_all_ini_files() -> Vec<String> {
        // TODO(php-runtime)
        // No XdebugHandler is ever constructed (`self::$name` stays null), because the
        // `new XdebugHandler('Composer'); $xdebug->check();` bootstrap in `bin/composer` is not
        // ported (see the TODO(phase-c) at the top of shirabe's main.rs), so the
        // COMPOSER_ORIGINAL_INIS env-var branch is unreachable here.
        //
        // Callers that need the real PHP runtime's ini files (php_ini_loaded_file() /
        // php_ini_scanned_files()) query shirabe_php_rpc directly instead of going through this
        // stub; see IniHelper::get_all in the shirabe crate.
        vec![String::new()]
    }
}
