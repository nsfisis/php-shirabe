//! ref: composer/src/Composer/Plugin/Capability/Capability.php

use crate::plugin::capability::CommandProvider;

/// Marker interface for Plugin capabilities. Every new Capability which is added to the
/// Plugin API must implement this interface.
///
/// The accessor replaces PHP's `instanceof` downcast on a capability instance of unknown
/// concrete type (the way `PluginInterface::as_capable` does for plugins).
pub trait Capability {
    fn as_command_provider(&self) -> Option<&dyn CommandProvider> {
        None
    }
}
