use super::console_event::ConsoleEvent;
use crate::symfony::console::command::command::Command;
use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::output::output_interface::OutputInterface;

#[derive(Debug)]
pub struct ConsoleSignalEvent {
    inner: ConsoleEvent,
    handling_signal: i64,
}

impl ConsoleSignalEvent {
    pub fn new(
        command: Box<dyn Command>,
        input: Box<dyn InputInterface>,
        output: Box<dyn OutputInterface>,
        handling_signal: i64,
    ) -> Self {
        Self {
            inner: ConsoleEvent::new(Some(command), input, output),
            handling_signal,
        }
    }

    pub fn get_handling_signal(&self) -> i64 {
        self.handling_signal
    }
}
