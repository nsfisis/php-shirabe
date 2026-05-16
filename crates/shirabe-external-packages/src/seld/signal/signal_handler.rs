#[derive(Debug)]
pub struct SignalHandler;

impl SignalHandler {
    pub const SIGINT: &'static str = "SIGINT";
    pub const SIGTERM: &'static str = "SIGTERM";
    pub const SIGHUP: &'static str = "SIGHUP";

    pub fn create(signals: Vec<String>, callback: Box<dyn Fn(String, &SignalHandler)>) -> Self {
        todo!()
    }

    pub fn unregister(&self) {
        todo!()
    }
}
