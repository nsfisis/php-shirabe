//! ref: composer/tests/Composer/Test/IO/BufferIOTest.php

use shirabe::io::buffer_io::BufferIO;
use shirabe::io::IOInterfaceImmutable;
use shirabe_external_packages::symfony::console::output::output_interface::VERBOSITY_NORMAL;
use shirabe_php_shim::PhpMixed;

#[test]
#[ignore = "BufferIO::set_user_inputs is todo!() (needs StreamableInputInterface downcast wiring)"]
fn test_set_user_inputs() {
    let mut buffer_io = BufferIO::new(String::new(), VERBOSITY_NORMAL, None).unwrap();

    // The Rust port always uses a StreamableInputInterface, so the version-guard
    // exception branch in the original test does not apply.
    buffer_io
        .set_user_inputs(vec!["yes".to_string(), "no".to_string(), String::new()])
        .unwrap();

    assert!(buffer_io.ask_confirmation("Please say yes!".to_string(), false));
    assert!(!buffer_io.ask_confirmation("Now please say no!".to_string(), true));
    assert_eq!(
        PhpMixed::String("default".to_string()),
        buffer_io.ask(
            "Empty string last".to_string(),
            PhpMixed::String("default".to_string())
        )
    );
}
