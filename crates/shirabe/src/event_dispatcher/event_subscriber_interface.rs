//! ref: composer/src/Composer/EventDispatcher/EventSubscriberInterface.php sha1:7f4a6ec660663030ead98fa0f6a545554ee63b7d

use crate::php_worker::proxy::{PhpPluginProxy, subscribed_events};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_binding_macros::php_binding;
use shirabe_php_rpc::PhpEntityHandle;

/// Represents one event's subscriber info: method name only, method+priority, or multiple handlers.
#[derive(Debug)]
pub enum SubscribedEventEntry {
    Method(BString),
    MethodWithPriority(BString, Option<i64>),
    Methods(Vec<(BString, Option<i64>)>),
}

// The sole implementor is the PHP plugin proxy (plugins are the only subscribers in Composer
// itself), so the trait deviates from the PHP shape in two deliberate ways: the PHP-side static
// `getSubscribedEvents()` takes `&self` here (the receiver carries which PHP class to call, and
// an associated function would not be dyn-compatible), and it is fallible because the answer
// crosses the RPC boundary.
#[php_binding(
    r"Composer\EventDispatcher\EventSubscriberInterface",
    proxy = PhpPluginProxy
)]
pub trait EventSubscriberInterface {
    /// Returns an array of event names this subscriber wants to listen to.
    #[php(custom = subscribed_events)]
    fn get_subscribed_events(&self) -> anyhow::Result<IndexMap<BString, SubscribedEventEntry>>;

    /// The subscriber as it is passed over RPC: PHP's `[$subscriber, $method]` array callables
    /// capture the subscriber object itself, represented here by its PHP entity handle.
    #[php(rust_only)]
    fn subscriber_handle(&self) -> PhpEntityHandle;
}
