use shirabe_php_shim::PhpMixed;

/// Mirror of PSR-14 / Symfony's `EventDispatcherInterface`.
pub trait EventDispatcherInterface: std::fmt::Debug {
    fn dispatch(&self, event: PhpMixed, event_name: &str) -> PhpMixed;
}
