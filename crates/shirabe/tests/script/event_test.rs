//! ref: composer/tests/Composer/Test/Script/EventTest.php

use indexmap::IndexMap;
use shirabe::composer::{ComposerHandle, PartialOrFullComposer};
use shirabe::config::Config;
use shirabe::event_dispatcher::Event as BaseEvent;
use shirabe::event_dispatcher::EventInterface;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::{RootPackageHandle, RootPackageInterfaceHandle};
use shirabe::script::{Event, OriginatingEvent};
use std::cell::RefCell;
use std::rc::Rc;

fn create_composer_instance() -> ComposerHandle {
    let composer =
        ComposerHandle::from_rc_unchecked(Rc::new(RefCell::new(PartialOrFullComposer::new_full())));
    let config = Rc::new(RefCell::new(Config::new(true, None)));
    composer.borrow_mut().set_config(config);
    let package: RootPackageInterfaceHandle = RootPackageHandle::new(
        "foo".to_string(),
        "1.0.0.0".to_string(),
        "1.0.0".to_string(),
    )
    .into();
    composer.borrow_mut().set_package(package);
    composer
}

#[test]
fn test_event_sets_originating_event() {
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let composer = create_composer_instance();

    let originating_event = BaseEvent::new("originatingEvent".to_string(), vec![], IndexMap::new());

    let mut script_event = Event::new(
        "test".to_string(),
        composer.downgrade(),
        io,
        true,
        vec![],
        IndexMap::new(),
    );

    assert!(
        script_event.get_originating_event().is_none(),
        "originatingEvent is initialized as null"
    );

    script_event.set_originating_event(OriginatingEvent::Base(originating_event));

    // assertSame: the originating event passed in is returned unchanged.
    assert_eq!(
        script_event
            .get_originating_event()
            .map(EventInterface::get_name),
        Some("originatingEvent"),
        "getOriginatingEvent() SHOULD return test event"
    );
}

#[test]
fn test_event_calculates_nested_originating_event() {
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let composer = create_composer_instance();

    let originating_event =
        BaseEvent::new("upperOriginatingEvent".to_string(), vec![], IndexMap::new());

    let mut intermediate_event = Event::new(
        "intermediate".to_string(),
        composer.downgrade(),
        io.clone(),
        true,
        vec![],
        IndexMap::new(),
    );
    intermediate_event.set_originating_event(OriginatingEvent::Base(originating_event));

    let mut script_event = Event::new(
        "test".to_string(),
        composer.downgrade(),
        io,
        true,
        vec![],
        IndexMap::new(),
    );
    script_event.set_originating_event(OriginatingEvent::Script(Box::new(intermediate_event)));

    // assertNotSame/assertSame: compare by name since Rust lacks PHP object identity.
    let name = script_event
        .get_originating_event()
        .map(EventInterface::get_name);
    assert_ne!(
        name,
        Some("intermediate"),
        "getOriginatingEvent() SHOULD NOT return intermediate events"
    );
    assert_eq!(
        name,
        Some("upperOriginatingEvent"),
        "getOriginatingEvent() SHOULD return upper-most event"
    );
}
