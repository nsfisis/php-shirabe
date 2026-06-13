//! ref: composer/vendor/symfony/console/Command/SignalableCommandInterface.php

/// Interface for command reacting to signal.
pub trait SignalableCommandInterface {
    /// Returns the list of signals to subscribe.
    fn get_subscribed_signals(&self) -> Vec<i64>;

    /// The method will be called when the application is signaled.
    fn handle_signal(&mut self, signal: i64);
}
