//! ref: composer/src/Composer/Plugin/PreCommandRunEvent.php

// TODO(plugin): this event is part of the plugin API and is dispatched before a command runs
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use crate::event_dispatcher::event::Event;

pub struct PreCommandRunEvent {
    inner: Event,
    input: Box<dyn InputInterface>,
    command: String,
}

impl PreCommandRunEvent {
    pub fn new(name: String, input: Box<dyn InputInterface>, command: String) -> Self {
        let inner = Event::new(name);
        Self {
            inner,
            input,
            command,
        }
    }

    pub fn get_input(&self) -> &dyn InputInterface {
        self.input.as_ref()
    }

    pub fn get_command(&self) -> &str {
        &self.command
    }
}
