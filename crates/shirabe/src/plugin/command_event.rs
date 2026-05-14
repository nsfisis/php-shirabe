//! ref: composer/src/Composer/Plugin/CommandEvent.php

use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use crate::event_dispatcher::event::Event;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct CommandEvent {
    inner: Event,
    command_name: String,
    input: Box<dyn InputInterface>,
    output: Box<dyn OutputInterface>,
}

impl CommandEvent {
    pub fn new(
        name: String,
        command_name: String,
        input: Box<dyn InputInterface>,
        output: Box<dyn OutputInterface>,
        args: Vec<PhpMixed>,
        flags: Vec<PhpMixed>,
    ) -> Self {
        let inner = Event::new(name, args, flags);
        Self { inner, command_name, input, output }
    }

    pub fn get_input(&self) -> &dyn InputInterface {
        self.input.as_ref()
    }

    pub fn get_output(&self) -> &dyn OutputInterface {
        self.output.as_ref()
    }

    pub fn get_command_name(&self) -> &str {
        &self.command_name
    }
}
