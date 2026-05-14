//! ref: composer/src/Composer/EventDispatcher/Event.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct Event {
    pub(crate) name: String,
    pub(crate) args: Vec<String>,
    pub(crate) flags: IndexMap<String, PhpMixed>,
    propagation_stopped: bool,
}

impl Event {
    pub fn new(name: String, args: Vec<String>, flags: IndexMap<String, PhpMixed>) -> Self {
        Self {
            name,
            args,
            flags,
            propagation_stopped: false,
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_arguments(&self) -> &Vec<String> {
        &self.args
    }

    pub fn get_flags(&self) -> &IndexMap<String, PhpMixed> {
        &self.flags
    }

    pub fn is_propagation_stopped(&self) -> bool {
        self.propagation_stopped
    }

    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
    }
}
