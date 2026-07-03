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
        todo!()
    }
}
