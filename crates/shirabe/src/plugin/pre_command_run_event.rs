//! ref: composer/src/Composer/Plugin/PreCommandRunEvent.php

// TODO(plugin): this event is part of the plugin API and is dispatched before a command runs
use crate::event_dispatcher::Event;
use crate::event_dispatcher::EventInterface;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct PreCommandRunEvent {
    inner: Event,
    input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
    command: String,
}

impl PreCommandRunEvent {
    pub fn new(
        name: String,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        command: String,
    ) -> Self {
        let inner = Event::new(name, vec![], indexmap::IndexMap::new());
        Self {
            inner,
            input,
            command,
        }
    }

    pub fn get_input(&self) -> std::rc::Rc<std::cell::RefCell<dyn InputInterface>> {
        self.input.clone()
    }

    pub fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    pub fn get_command(&self) -> &str {
        &self.command
    }
}

impl EventInterface for PreCommandRunEvent {
    fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    fn get_arguments(&self) -> &Vec<String> {
        self.inner.get_arguments()
    }

    fn get_flags(&self) -> &IndexMap<String, PhpMixed> {
        self.inner.get_flags()
    }

    fn is_propagation_stopped(&self) -> bool {
        self.inner.is_propagation_stopped()
    }

    fn stop_propagation(&mut self) {
        self.inner.stop_propagation();
    }
}
