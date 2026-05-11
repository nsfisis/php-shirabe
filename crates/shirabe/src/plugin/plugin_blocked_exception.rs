//! ref: composer/src/Composer/Plugin/PluginBlockedException.php

use shirabe_php_shim::UnexpectedValueException;

// TODO(plugin): PluginBlockedException is a part of Plugin API.
pub struct PluginBlockedException(pub UnexpectedValueException);
