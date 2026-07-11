//! ref: composer/tests/Composer/Test/Installer/InstallerEventTest.php

use shirabe::composer::{ComposerHandle, PartialOrFullComposer};
use shirabe::dependency_resolver::Transaction;
use shirabe::event_dispatcher::EventInterface;
use shirabe::installer::InstallerEvent;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;

#[test]
fn test_getter() {
    let composer_rc = std::rc::Rc::new(std::cell::RefCell::new(PartialOrFullComposer::new_full()));
    let composer = ComposerHandle::from_rc_unchecked(composer_rc);
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let transaction = Transaction::new(vec![], vec![]);
    let event = InstallerEvent::new(
        "EVENT_NAME".to_string(),
        composer.downgrade(),
        io,
        true,
        true,
        transaction,
    );

    assert_eq!("EVENT_NAME", event.get_name());
    assert!(event.is_dev_mode());
    assert!(event.is_executing_operations());
    assert!(event.get_transaction().is_some());
}
