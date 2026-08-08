//! ref: composer/src/Composer/Plugin/PluginBlockedException.php

use shirabe_php_shim::UnexpectedValueException;

// TODO(plugin): PluginBlockedException is a part of Plugin API.
#[derive(Debug)]
pub struct PluginBlockedException(pub UnexpectedValueException);

impl PluginBlockedException {
    pub fn new(message: String) -> Self {
        Self(UnexpectedValueException::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(
    PluginBlockedException,
    0,
    r"Composer\Plugin\PluginBlockedException"
);
