//! ref: composer/src/Composer/Plugin/CommandEvent.php

use crate::event_dispatcher::event::Event;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::component::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::component::console::output::output_interface::OutputInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct CommandEvent {
    inner: Event,
    command_name: String,
}

impl CommandEvent {
    // TODO(phase-b): input/output dropped because storing &dyn references in an event would
    // require lifetime parameters; restore once Plugin API needs them.
    pub fn new(
        name: &str,
        command_name: &str,
        _input: &dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> Self {
        Self::new6(name, command_name, _input, _output, vec![], IndexMap::new())
    }

    pub fn new6(
        name: &str,
        command_name: &str,
        _input: &dyn InputInterface,
        _output: &dyn OutputInterface,
        args: Vec<String>,
        flags: IndexMap<String, PhpMixed>,
    ) -> Self {
        let inner = Event::new(name.to_string(), args, flags);
        Self {
            inner,
            command_name: command_name.to_string(),
        }
    }

    pub fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    pub fn get_command_name(&self) -> &str {
        &self.command_name
    }
}
