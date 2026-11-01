//! ref: composer/src/Composer/Plugin/CommandEvent.php

use crate::event_dispatcher::{Event, EventInterface};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;
use shirabe_symfony_console::input::InputInterface;
use shirabe_symfony_console::output::OutputInterface;

#[derive(Debug)]
pub struct CommandEvent {
    inner: Event,
    command_name: BString,
    input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
    output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
}

impl CommandEvent {
    pub fn new(
        name: &str,
        command_name: impl AsRef<[u8]>,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> Self {
        Self::new6(name, command_name, input, output, vec![], IndexMap::new())
    }

    pub fn new6(
        name: &str,
        command_name: impl AsRef<[u8]>,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        args: Vec<BString>,
        flags: IndexMap<BString, PhpMixed>,
    ) -> Self {
        let inner = Event::new(name.to_string(), args, flags);
        Self {
            inner,
            command_name: BString::from(command_name.as_ref()),
            input,
            output,
        }
    }

    pub fn get_input(&self) -> std::rc::Rc<std::cell::RefCell<dyn InputInterface>> {
        self.input.clone()
    }

    pub fn get_output(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> {
        self.output.clone()
    }

    pub fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    pub fn get_command_name(&self) -> &bstr::BStr {
        self.command_name.as_ref()
    }
}

impl EventInterface for CommandEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn get_name(&self) -> &str {
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
