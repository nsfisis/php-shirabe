//! ref: composer/src/Composer/Plugin/PluginBlockedException.php sha1:aa7b95b50977653112c523868c37453e8523d220

use bstr::BString;
use shirabe_php_shim::{UnexpectedValueException, impl_php_exception};

// TODO(plugin): PluginBlockedException is a part of Plugin API.
#[derive(Debug)]
pub struct PluginBlockedException(pub UnexpectedValueException);

impl PluginBlockedException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self(UnexpectedValueException::new(message))
    }
}

impl_php_exception!(
    PluginBlockedException,
    0,
    r"Composer\Plugin\PluginBlockedException"
);
