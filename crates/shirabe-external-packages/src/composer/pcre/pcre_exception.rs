//! ref: composer/vendor/composer/pcre/src/PcreException.php

use super::preg::{preg_last_error, preg_last_error_msg};

#[derive(Debug)]
pub struct PcreException(pub shirabe_php_shim::RuntimeException);

impl PcreException {
    pub fn from_function(function: &str, pattern: &str) -> PcreException {
        let code = preg_last_error();

        PcreException(shirabe_php_shim::RuntimeException {
            message: format!(
                "{}(): failed executing \"{}\": {}",
                function,
                pattern,
                Self::pcre_last_error_message(code)
            ),
            code,
        })
    }

    // Modern PHP always provides preg_last_error_msg(), so the legacy fallbacks
    // that scanned the pcre constants are not reproduced.
    fn pcre_last_error_message(_code: i64) -> String {
        preg_last_error_msg()
    }
}

impl std::fmt::Display for PcreException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for PcreException {}
