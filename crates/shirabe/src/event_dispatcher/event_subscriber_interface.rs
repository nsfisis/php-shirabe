//! ref: composer/src/Composer/EventDispatcher/EventSubscriberInterface.php

use indexmap::IndexMap;
use shirabe_php_rpc::PhpObjHandle;

/// Represents one event's subscriber info: method name only, method+priority, or multiple handlers.
#[derive(Debug)]
pub enum SubscribedEventEntry {
    Method(String),
    MethodWithPriority(String, Option<i64>),
    Methods(Vec<(String, Option<i64>)>),
}

// The sole implementor is the PHP plugin proxy (plugins are the only subscribers in Composer
// itself), so the trait deviates from the PHP shape in two deliberate ways: the PHP-side static
// `getSubscribedEvents()` takes `&self` here (the receiver carries which PHP class to call, and
// an associated function would not be dyn-compatible), and it is fallible because the answer
// crosses the RPC boundary.
pub trait EventSubscriberInterface {
    /// Returns an array of event names this subscriber wants to listen to.
    fn get_subscribed_events(&self) -> anyhow::Result<IndexMap<String, SubscribedEventEntry>>;

    /// The subscriber as it crosses the wire: PHP's `[$subscriber, $method]` array callables
    /// capture the subscriber object itself, represented here by its P-table handle.
    fn subscriber_handle(&self) -> PhpObjHandle;
}
