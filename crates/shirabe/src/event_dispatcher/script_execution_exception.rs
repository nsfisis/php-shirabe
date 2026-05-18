//! ref: composer/src/Composer/EventDispatcher/ScriptExecutionException.php

use shirabe_php_shim::RuntimeException;

/// Thrown when a script running an external process exits with a non-0 status code
#[derive(Debug)]
pub struct ScriptExecutionException(pub RuntimeException);

impl ScriptExecutionException {
    pub fn get_code(&self) -> i64 {
        self.0.code
    }

    pub fn get_message(&self) -> &str {
        &self.0.message
    }
}

impl std::fmt::Display for ScriptExecutionException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for ScriptExecutionException {}
