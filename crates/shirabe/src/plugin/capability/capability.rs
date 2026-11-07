//! ref: composer/src/Composer/Plugin/Capability/Capability.php sha1:bd10fce988e656db9373e45b72aca5feaa748265

use crate::plugin::PhpCapabilityProxy;
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

    /// For testing only: recovers the PHP-backed proxy so tests can read capability properties
    /// the way PHPUnit asserts `$capability->args`.
    fn __as_php_capability_proxy(&self) -> Option<&PhpCapabilityProxy> {
        None
    }
}
