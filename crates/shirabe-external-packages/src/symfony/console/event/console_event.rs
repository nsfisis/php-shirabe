//! ref: composer/vendor/symfony/console/Event/ConsoleEvent.php

use crate::symfony::console::command::command::Command;
use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::output::output_interface::OutputInterface;
use crate::symfony::contracts::event_dispatcher::event::Event;

/// Allows to inspect input and output of a command.
#[derive(Debug)]
pub struct ConsoleEvent {
    inner: Event,
    pub(crate) command: Option<Box<dyn Command>>,
    input: Box<dyn InputInterface>,
    output: Box<dyn OutputInterface>,
}

impl ConsoleEvent {
    pub fn new(
        command: Option<Box<dyn Command>>,
        input: Box<dyn InputInterface>,
        output: Box<dyn OutputInterface>,
    ) -> Self {
        Self {
            inner: Event,
            command,
            input,
            output,
        }
    }

    pub fn get_command(&self) -> Option<&dyn Command> {
        self.command.as_deref()
    }

    pub fn get_input(&self) -> &dyn InputInterface {
        self.input.as_ref()
    }

    pub fn get_output(&self) -> &dyn OutputInterface {
        self.output.as_ref()
    }
}
