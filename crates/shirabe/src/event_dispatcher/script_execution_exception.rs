//! ref: composer/src/Composer/EventDispatcher/ScriptExecutionException.php

use shirabe_php_shim::RuntimeException;

/// Thrown when a script running an external process exits with a non-0 status code
#[derive(Debug)]
pub struct ScriptExecutionException(pub RuntimeException);
