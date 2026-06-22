//! ref: composer/tests/Composer/Test/Script/EventTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::composer::{ComposerHandle, PartialOrFullComposer};
use shirabe::config::Config;
use shirabe::event_dispatcher::Event as BaseEvent;
use shirabe::event_dispatcher::EventInterface;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::{RootPackageHandle, RootPackageInterfaceHandle};
use shirabe::script::Event;

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
#[ignore]
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

    script_event.set_originating_event(originating_event);

    // assertSame: the originating event passed in is returned unchanged.
    assert_eq!(
        script_event
            .get_originating_event()
            .map(EventInterface::get_name),
        Some("originatingEvent"),
        "getOriginatingEvent() SHOULD return test event"
    );
}

// In PHP, the intermediate originating event is itself a Script\Event, and
// getOriginatingEvent() recurses to return the upper-most event. Here
// set_originating_event takes a concrete event_dispatcher::Event, so a Script
// Event cannot be passed as the originating event (it is not polymorphic), and
// the test cannot be expressed faithfully.
#[test]
#[ignore = "Event::set_originating_event takes a concrete event_dispatcher::Event, so a nested script::Event cannot be passed as originating event; the recursive calculate_originating_event behavior is not expressible"]
fn test_event_calculates_nested_originating_event() {
    todo!()
}
