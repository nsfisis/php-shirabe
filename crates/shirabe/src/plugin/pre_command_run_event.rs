//! ref: composer/src/Composer/Plugin/PreCommandRunEvent.php

// TODO(plugin): this event is part of the plugin API and is dispatched before a command runs
use crate::event_dispatcher::event::Event;
use shirabe_external_packages::symfony::component::console::input::input_interface::InputInterface;

#[derive(Debug)]
pub struct PreCommandRunEvent {
    inner: Event,
    command: String,
}

impl PreCommandRunEvent {
    // TODO(phase-b): input dropped because storing a &dyn reference would need lifetime params.
    pub fn new(name: String, _input: &dyn InputInterface, command: String) -> Self {
        let inner = Event::new(name, vec![], indexmap::IndexMap::new());
        Self { inner, command }
    }

    pub fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    pub fn get_command(&self) -> &str {
        &self.command
    }
}
