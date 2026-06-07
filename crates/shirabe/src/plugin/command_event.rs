//! ref: composer/src/Composer/Plugin/CommandEvent.php

use crate::event_dispatcher::Event;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct CommandEvent {
    inner: Event,
    command_name: String,
    input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
    output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
}

impl CommandEvent {
    pub fn new(
        name: &str,
        command_name: &str,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> Self {
        Self::new6(name, command_name, input, output, vec![], IndexMap::new())
    }

    pub fn new6(
        name: &str,
        command_name: &str,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        args: Vec<String>,
        flags: IndexMap<String, PhpMixed>,
    ) -> Self {
        let inner = Event::new(name.to_string(), args, flags);
        Self {
            inner,
            command_name: command_name.to_string(),
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

    pub fn get_command_name(&self) -> &str {
        &self.command_name
    }
}
