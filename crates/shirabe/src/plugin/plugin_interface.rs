//! ref: composer/src/Composer/Plugin/PluginInterface.php

use crate::composer::ComposerHandle;
use crate::io::IOInterface;
use crate::plugin::Capable;

pub const PLUGIN_API_VERSION: &str = "2.9.0";

pub trait PluginInterface: std::fmt::Debug {
    fn activate(
        &mut self,
        composer: &ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    );

    fn deactivate(
        &mut self,
        composer: &ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    );

    fn uninstall(
        &mut self,
        composer: &ComposerHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    );

    // TODO(plugin): PHP-side `instanceof` checks for EventSubscriberInterface / Capable.
    // EventSubscriberInterface is not dyn-compatible (its only method is associated, not
    // a `&self` method), so we expose a boolean predicate instead.
    fn is_event_subscriber_interface(&self) -> bool {
        false
    }

    fn as_capable(&self) -> Option<&dyn Capable> {
        None
    }
}
