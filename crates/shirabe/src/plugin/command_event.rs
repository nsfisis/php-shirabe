//! ref: composer/src/Composer/Plugin/CommandEvent.php sha1:49a67f60d60f95d2a6013eee5970f90d4c57b01a

use crate::event_dispatcher::{Event, EventFlags, delegate_event_interface_to_inner};
use bstr::BString;
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
        name: impl Into<BString>,
        command_name: impl AsRef<[u8]>,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> Self {
        Self::with_args_and_flags(
            name,
            command_name,
            input,
            output,
            vec![],
            EventFlags::default(),
        )
    }

    pub fn with_args_and_flags(
        name: impl Into<BString>,
        command_name: impl AsRef<[u8]>,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        args: Vec<BString>,
        flags: EventFlags,
    ) -> Self {
        let inner = Event::new(name, args, flags);
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

    pub fn get_name(&self) -> &bstr::BStr {
        self.inner.get_name()
    }

    pub fn get_command_name(&self) -> &bstr::BStr {
        self.command_name.as_ref()
    }
}

delegate_event_interface_to_inner!(CommandEvent, inner);
