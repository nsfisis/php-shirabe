//! ref: composer/src/Composer/Script/Event.php

use crate::composer::ComposerWeakHandle;
use crate::event_dispatcher::Event as BaseEvent;
use crate::event_dispatcher::EventInterface;
use crate::io::IOInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct Event {
    inner: BaseEvent,
    composer: ComposerWeakHandle,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    dev_mode: bool,
    originating_event: Option<Box<BaseEvent>>,
}

impl Event {
    pub fn new(
        name: String,
        composer: ComposerWeakHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        dev_mode: bool,
        args: Vec<String>,
        flags: IndexMap<String, PhpMixed>,
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

    pub fn get_io(&self) -> std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
        self.io.clone()
    }

    pub fn is_dev_mode(&self) -> bool {
        self.dev_mode
    }

    pub fn get_originating_event(&self) -> Option<&BaseEvent> {
        self.originating_event.as_deref()
    }

    pub fn set_originating_event(&mut self, event: BaseEvent) -> &mut Self {
        self.originating_event = Some(Box::new(self.calculate_originating_event(event)));
        self
    }

    fn calculate_originating_event(&self, event: BaseEvent) -> BaseEvent {
        // if ($event instanceof Event && $event->getOriginatingEvent()) {
        //     return $this->calculateOriginatingEvent($event->getOriginatingEvent());
        // }
        //
        // return $event;
        todo!()
    }
}

impl EventInterface for Event {
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
