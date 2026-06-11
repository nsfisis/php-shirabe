use super::console_event::ConsoleEvent;
use crate::symfony::console::command::command::Command;
use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::output::output_interface::OutputInterface;

/// Allows to manipulate the exit code of a command after its execution.
#[derive(Debug)]
pub struct ConsoleTerminateEvent {
    inner: ConsoleEvent,
    exit_code: i64,
}

impl ConsoleTerminateEvent {
    pub fn new(
        command: Box<dyn Command>,
        input: Box<dyn InputInterface>,
        output: Box<dyn OutputInterface>,
        exit_code: i64,
    ) -> Self {
        let mut instance = Self {
            inner: ConsoleEvent::new(Some(command), input, output),
            exit_code: 0,
        };
        instance.set_exit_code(exit_code);
        instance
    }

    pub fn set_exit_code(&mut self, exit_code: i64) {
        self.exit_code = exit_code;
    }

    pub fn get_exit_code(&self) -> i64 {
        self.exit_code
    }
}
