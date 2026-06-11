use super::console_event::ConsoleEvent;
use crate::symfony::console::command::command::Command;
use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::output::output_interface::OutputInterface;

/// Allows to do things before the command is executed, like skipping the command or
/// executing code before the command is going to be executed.
///
/// Changing the input arguments will have no effect.
#[derive(Debug)]
pub struct ConsoleCommandEvent {
    inner: ConsoleEvent,
    command_should_run: bool,
}

impl ConsoleCommandEvent {
    pub const RETURN_CODE_DISABLED: i64 = 113;

    pub fn new(
        command: Option<Box<dyn Command>>,
        input: Box<dyn InputInterface>,
        output: Box<dyn OutputInterface>,
    ) -> Self {
        Self {
            inner: ConsoleEvent::new(command, input, output),
            command_should_run: true,
        }
    }

    pub fn disable_command(&mut self) -> bool {
        self.command_should_run = false;
        self.command_should_run
    }

    pub fn enable_command(&mut self) -> bool {
        self.command_should_run = true;
        self.command_should_run
    }

    pub fn command_should_run(&self) -> bool {
        self.command_should_run
    }
}
