//! ref: composer/src/Composer/Plugin/PreCommandRunEvent.php

use crate::event_dispatcher::{Event, EventInterface};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;
use shirabe_symfony_console::input::InputInterface;

#[derive(Debug)]
pub struct PreCommandRunEvent {
    inner: Event,
    input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
    command: BString,
}

impl PreCommandRunEvent {
    pub fn new(
        name: impl Into<BString>,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        command: BString,
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

    pub fn get_name(&self) -> &bstr::BStr {
        self.inner.get_name()
    }

    pub fn get_command(&self) -> &bstr::BStr {
        self.command.as_ref()
    }
}

impl EventInterface for PreCommandRunEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn get_name(&self) -> &bstr::BStr {
        self.inner.get_name()
    }

    fn get_arguments(&self) -> &Vec<bstr::BString> {
        self.inner.get_arguments()
    }

    fn get_flags(&self) -> &IndexMap<BString, PhpMixed> {
        self.inner.get_flags()
    }

    fn is_propagation_stopped(&self) -> bool {
        self.inner.is_propagation_stopped()
    }

    fn stop_propagation(&mut self) {
        self.inner.stop_propagation();
    }
}
