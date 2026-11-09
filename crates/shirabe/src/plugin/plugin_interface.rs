//! ref: composer/src/Composer/Plugin/PluginInterface.php sha1:83f0931d0114bbfd41a05d0834b2e7ca4efe07d1

use crate::composer::ComposerHandle;
use crate::event_dispatcher::EventSubscriberInterface;
use crate::io::IOInterface;
use crate::php_worker::proxy::PhpPluginProxy;
use crate::plugin::Capable;
use bstr::{BStr, BString};
use shirabe_php_shim::php_str;

pub const PLUGIN_API_VERSION: &BStr = php_str(b"2.9.0");

pub trait PluginInterface: std::fmt::Debug {
    // The PHP methods return void but may throw; the owned `ComposerHandle` follows the shared
    // handle policy — plugins typically store `$composer` beyond the call.
    fn activate(
        &mut self,
        composer: ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()>;

    fn deactivate(
        &mut self,
        composer: ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()>;

    fn uninstall(
        &mut self,
        composer: ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<()>;

    /// PHP: `get_class($plugin)`. Rust has no runtime class name, so each implementor carries
    /// the name PHP would report.
    fn get_class_name(&self) -> BString;

    // PHP-side `instanceof EventSubscriberInterface` / `instanceof Capable` checks map to
    // these downcast accessors.
    fn as_event_subscriber(&self) -> Option<&dyn EventSubscriberInterface> {
        None
    }

    fn as_capable(&self) -> Option<&dyn Capable> {
        None
    }

    /// For testing only: recovers the PHP-backed proxy so tests can read plugin properties the
    /// way PHPUnit asserts `$plugins[0]->version`.
    fn __as_php_plugin_proxy(&self) -> Option<&PhpPluginProxy> {
        None
    }
}
