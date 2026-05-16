//! ref: composer/src/Composer/Script/Event.php

use crate::composer::Composer;
use crate::event_dispatcher::event::Event as BaseEvent;
use crate::io::io_interface::IOInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct Event {
    inner: BaseEvent,
    composer: Composer,
    io: Box<dyn IOInterface>,
    dev_mode: bool,
    originating_event: Option<Box<BaseEvent>>,
}

impl Event {
    pub fn new(
        name: String,
        composer: Composer,
        io: Box<dyn IOInterface>,
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

    pub fn get_composer(&self) -> &Composer {
        &self.composer
    }

    pub fn get_io(&self) -> &dyn IOInterface {
        self.io.as_ref()
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
