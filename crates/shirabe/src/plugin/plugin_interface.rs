//! ref: composer/src/Composer/Plugin/PluginInterface.php

use crate::composer::ComposerHandle;
use crate::io::IOInterface;
use crate::plugin::Capable;

pub const PLUGIN_API_VERSION: &str = "2.9.0";

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
    fn get_class_name(&self) -> String;

    // TODO(plugin): PHP-side `instanceof` checks for EventSubscriberInterface / Capable.
    // EventSubscriberInterface is not dyn-compatible (its only method is associated, not
    // a `&self` method), so we expose a boolean predicate instead.
    fn is_event_subscriber_interface(&self) -> bool {
        false
    }

    fn as_capable(&self) -> Option<&dyn Capable> {
        None
    }

    /// For testing only: recovers the PHP-backed proxy so tests can read plugin properties the
    /// way PHPUnit asserts `$plugins[0]->version`.
    fn __as_php_plugin_proxy(&self) -> Option<&crate::plugin::PhpPluginProxy> {
        None
    }
}
