//! ref: composer/src/Composer/EventDispatcher/EventSubscriberInterface.php

use indexmap::IndexMap;

/// Represents one event's subscriber info: method name only, method+priority, or multiple handlers.
pub enum SubscribedEventEntry {
    Method(String),
    MethodWithPriority(String, Option<i64>),
    Methods(Vec<(String, Option<i64>)>),
}

pub trait EventSubscriberInterface {
    /// Returns an array of event names this subscriber wants to listen to.
    fn get_subscribed_events() -> IndexMap<String, SubscribedEventEntry>;
}
