//! ref: composer/src/Composer/Plugin/PluginBlockedException.php

use bstr::BString;
use shirabe_php_shim::UnexpectedValueException;

// TODO(plugin): PluginBlockedException is a part of Plugin API.
#[derive(Debug)]
pub struct PluginBlockedException(pub UnexpectedValueException);

impl PluginBlockedException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self(UnexpectedValueException::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(
    PluginBlockedException,
    0,
    r"Composer\Plugin\PluginBlockedException"
);
