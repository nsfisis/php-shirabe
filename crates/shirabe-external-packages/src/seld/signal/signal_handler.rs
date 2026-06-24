//! ref: composer/vendor/seld/signal-handler/src/SignalHandler.php

#[derive(Debug)]
pub struct SignalHandler;

// TODO(phase-d): disable signal handler at all for now.
impl SignalHandler {
    pub const SIGINT: &'static str = "SIGINT";
    pub const SIGTERM: &'static str = "SIGTERM";
    pub const SIGHUP: &'static str = "SIGHUP";

    pub fn create(_signals: Vec<String>, _callback: Box<dyn Fn(String, &SignalHandler)>) -> Self {
        Self
    }

    pub fn unregister(&self) {}

    pub fn exit_with_last_signal(&self) {
        std::process::exit(0);
    }

    pub fn is_triggered(&self) -> bool {
        false
    }
}
