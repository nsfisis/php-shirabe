//! ref: composer/src/Composer/EventDispatcher/ScriptExecutionException.php

use shirabe_php_shim::RuntimeException;

/// Thrown when a script running an external process exits with a non-0 status code
#[derive(Debug)]
pub struct ScriptExecutionException(pub RuntimeException);

impl ScriptExecutionException {
    pub fn new(message: String, code: i64) -> Self {
        Self(RuntimeException::with_code(message, code))
    }
}

shirabe_php_shim::impl_php_exception!(
    ScriptExecutionException,
    0,
    r"Composer\EventDispatcher\ScriptExecutionException"
);
