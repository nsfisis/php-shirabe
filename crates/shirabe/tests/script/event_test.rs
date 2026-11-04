//! ref: composer/tests/Composer/Test/Script/EventTest.php sha1:4bfce5295f319f68461552f61bfe338f670cbbf1

use bstr::BString;
use shirabe::composer::{ComposerHandle, PartialOrFullComposer};
use shirabe::config::Config;
use shirabe::event_dispatcher::{Event as BaseEvent, EventFlags, EventInterface};
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::{RootPackageHandle, RootPackageInterfaceHandle};
use shirabe::script::{Event, OriginatingEvent};

fn create_composer_instance() -> ComposerHandle {
    let composer = ComposerHandle::from_rc_unchecked(std::rc::Rc::new(std::cell::RefCell::new(
        PartialOrFullComposer::new_full(),
    )));
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(true, None)));
    composer.borrow_mut().set_config(config);
    let package: RootPackageInterfaceHandle = RootPackageHandle::new(
        BString::from("foo"),
        BString::from("1.0.0.0"),
        BString::from("1.0.0"),
    )
    .into();
    composer.borrow_mut().set_package(package);
    composer
}

#[test]
fn test_event_sets_originating_event() {
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let composer = create_composer_instance();

    let originating_event = BaseEvent::new(
        BString::from("originatingEvent"),
        vec![],
        EventFlags::default(),
    );

    let mut script_event = Event::new(
        BString::from("test"),
        composer.downgrade(),
        io,
        true,
        vec![],
        EventFlags::default(),
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
        Some("originatingEvent".into()),
        "getOriginatingEvent() SHOULD return test event"
    );
}

#[test]
fn test_event_calculates_nested_originating_event() {
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let composer = create_composer_instance();

    let originating_event = BaseEvent::new(
        BString::from("upperOriginatingEvent"),
        vec![],
        EventFlags::default(),
    );

    let mut intermediate_event = Event::new(
        BString::from("intermediate"),
        composer.downgrade(),
        io.clone(),
        true,
        vec![],
        EventFlags::default(),
    );
    intermediate_event.set_originating_event(OriginatingEvent::Base(originating_event));

    let mut script_event = Event::new(
        BString::from("test"),
        composer.downgrade(),
        io,
        true,
        vec![],
        EventFlags::default(),
    );
    script_event.set_originating_event(OriginatingEvent::Script(Box::new(intermediate_event)));

    // assertNotSame/assertSame: compare by name since Rust lacks PHP object identity.
    let name = script_event
        .get_originating_event()
        .map(EventInterface::get_name);
    assert_ne!(
        name,
        Some("intermediate".into()),
        "getOriginatingEvent() SHOULD NOT return intermediate events"
    );
    assert_eq!(
        name,
        Some("upperOriginatingEvent".into()),
        "getOriginatingEvent() SHOULD return upper-most event"
    );
}
