//! ref: composer/src/Composer/Script/Event.php sha1:c8dacd8d044ed5c6cef8cc1d32da2162894f6f3a

use crate::composer::ComposerWeakHandle;
use crate::event_dispatcher::{
    Event as BaseEvent, EventFlags, EventInterface, delegate_event_interface_to_inner,
};
use crate::io::IOInterface;
use bstr::BString;
use shirabe_php_binding_macros::php_binding;

#[derive(Debug)]
pub struct Event {
    inner: BaseEvent,
    composer: ComposerWeakHandle,
    io: std::rc::Rc<dyn IOInterface>,
    dev_mode: bool,
    originating_event: Option<OriginatingEvent>,
}

#[derive(Debug)]
pub enum OriginatingEvent {
    Base(BaseEvent),
    Script(Box<Event>),
}

#[php_binding(r"Composer\Script\Event", members = instance)]
impl Event {
    pub fn new(
        name: impl Into<BString>,
        composer: ComposerWeakHandle,
        io: std::rc::Rc<dyn IOInterface>,
        dev_mode: bool,
        args: Vec<BString>,
        flags: EventFlags,
    ) -> Self {
        Self {
            inner: BaseEvent::new(name, args, flags),
            composer,
            io,
            dev_mode,
            originating_event: None,
        }
    }

    pub fn get_composer(&self) -> &ComposerWeakHandle {
        &self.composer
    }

    pub fn get_io(&self) -> std::rc::Rc<dyn IOInterface> {
        self.io.clone()
    }

    pub fn is_dev_mode(&self) -> bool {
        self.dev_mode
    }

    #[php(skip)]
    pub fn get_originating_event(&self) -> Option<&OriginatingEvent> {
        self.originating_event.as_ref()
    }

    #[php(skip)]
    pub fn set_originating_event(&mut self, event: OriginatingEvent) -> &mut Self {
        self.originating_event = Some(Self::calculate_originating_event(event));
        self
    }

    fn calculate_originating_event(event: OriginatingEvent) -> OriginatingEvent {
        if let OriginatingEvent::Script(boxed) = event {
            let mut inner_event = *boxed;
            if let Some(originating) = inner_event.originating_event.take() {
                return Self::calculate_originating_event(originating);
            }
            return OriginatingEvent::Script(Box::new(inner_event));
        }

        event
    }
}

delegate_event_interface_to_inner!(Event, inner, "Composer\\Script\\Event");

impl EventInterface for OriginatingEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    // A Rust-side wrapper of the event a script event was dispatched for, with no proxy stub.
    fn php_class(&self) -> Option<&'static bstr::BStr> {
        None
    }

    fn get_name(&self) -> &bstr::BStr {
        match self {
            OriginatingEvent::Base(e) => e.get_name(),
            OriginatingEvent::Script(e) => e.get_name(),
        }
    }

    fn get_arguments(&self) -> &Vec<bstr::BString> {
        match self {
            OriginatingEvent::Base(e) => e.get_arguments(),
            OriginatingEvent::Script(e) => e.get_arguments(),
        }
    }

    fn get_flags(&self) -> &EventFlags {
        match self {
            OriginatingEvent::Base(e) => e.get_flags(),
            OriginatingEvent::Script(e) => e.get_flags(),
        }
    }

    fn is_propagation_stopped(&self) -> bool {
        match self {
            OriginatingEvent::Base(e) => e.is_propagation_stopped(),
            OriginatingEvent::Script(e) => e.is_propagation_stopped(),
        }
    }

    fn stop_propagation(&mut self) {
        match self {
            OriginatingEvent::Base(e) => e.stop_propagation(),
            OriginatingEvent::Script(e) => e.stop_propagation(),
        }
    }
}
