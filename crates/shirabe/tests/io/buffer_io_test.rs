//! ref: composer/tests/Composer/Test/IO/BufferIOTest.php sha1:182a18be83ead9e2a60c96143b9baee30292a800

use bstr::BString;
use shirabe::io::{BufferIO, IOInterfaceWrite};
use shirabe_symfony_console::input::InputValue;
use shirabe_symfony_console::output::VERBOSITY_NORMAL;

#[test]
fn test_set_user_inputs() {
    let buffer_io = BufferIO::new(BString::default(), VERBOSITY_NORMAL, None).unwrap();

    // The Rust port always uses a StreamableInputInterface, so the version-guard
    // exception branch in the original test does not apply.
    buffer_io
        .set_user_inputs(vec![
            BString::from("yes"),
            BString::from("no"),
            BString::default(),
        ])
        .unwrap();

    assert!(
        buffer_io
            .ask_confirmation("Please say yes!", false)
            .unwrap()
    );
    assert!(
        !buffer_io
            .ask_confirmation("Now please say no!", true)
            .unwrap()
    );
    assert_eq!(
        InputValue::String(BString::from("default")),
        buffer_io
            .ask(
                "Empty string last",
                InputValue::String(BString::from("default"))
            )
            .unwrap()
    );
}
