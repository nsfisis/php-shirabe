//! ref: composer/src/Composer/Plugin/PreCommandRunEvent.php sha1:0d6e0f6ec661b502b6461814365548d5e52fb136

use crate::event_dispatcher::{Event, EventFlags};
use bstr::BString;
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
        let inner = Event::new(name, vec![], EventFlags::default());
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

crate::event_dispatcher::delegate_event_interface_to_inner!(PreCommandRunEvent, inner);
