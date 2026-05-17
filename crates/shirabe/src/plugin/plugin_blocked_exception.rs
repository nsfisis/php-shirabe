//! ref: composer/src/Composer/Plugin/PluginBlockedException.php

use shirabe_php_shim::UnexpectedValueException;

// TODO(plugin): PluginBlockedException is a part of Plugin API.
pub struct PluginBlockedException(pub UnexpectedValueException);

impl std::fmt::Display for PluginBlockedException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for PluginBlockedException {}
