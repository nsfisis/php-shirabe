//! ref: composer/tests/Composer/Test/IO/ConsoleIOTest.php

// These mock Symfony's InputInterface/OutputInterface/HelperSet (and QuestionHelper) to
// drive ConsoleIO; those console abstractions are not ported.

use indexmap::IndexMap;
use shirabe::io::ConsoleIO;
use shirabe::io::IOInterfaceImmutable;
use shirabe::io::IOInterfaceMutable;
use shirabe_external_packages::symfony::console::helper::HelperSet;
use shirabe_external_packages::symfony::console::input::array_input::ArrayInput;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::buffered_output::BufferedOutput;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use std::cell::RefCell;
use std::rc::Rc;

/// Builds a ConsoleIO backed by real, side-effect-free Input/Output/HelperSet. PHP uses
/// PHPUnit mocks here, but for tests that exercise only the authentication map they carry no
/// expectations, so concrete implementations are an exact substitute.
fn make_console_io() -> ConsoleIO {
    let input: Rc<RefCell<dyn InputInterface>> =
        Rc::new(RefCell::new(ArrayInput::new(vec![], None).unwrap()));
    let output: Rc<RefCell<dyn OutputInterface>> =
        Rc::new(RefCell::new(BufferedOutput::new(None, false, None)));
    let helper_set = HelperSet::default();
    ConsoleIO::new(input, output, helper_set)
}

#[ignore = "requires a PHPUnit mock of InputInterface::isInteractive with willReturnOnConsecutiveCalls; no mocking framework"]
#[test]
fn test_is_interactive() {
    todo!()
}

#[ignore = "requires a PHPUnit mock of OutputInterface with expects()->method('write')->with() expectation verification; no mocking framework"]
#[test]
fn test_write() {
    todo!()
}

#[ignore = "requires a PHPUnit mock of ConsoleOutputInterface with getErrorOutput/write expectation verification; no mocking framework"]
#[test]
fn test_write_error() {
    todo!()
}

#[ignore = "requires a PHPUnit mock of OutputInterface with a write() callback expectation verification; no mocking framework"]
#[test]
fn test_write_with_multiple_line_string_when_debugging() {
    todo!()
}

#[ignore = "requires a PHPUnit mock of OutputInterface with willReturnCallback series expectation verification; no mocking framework"]
#[test]
fn test_overwrite() {
    todo!()
}

#[ignore = "requires a PHPUnit mock of QuestionHelper/HelperSet with ask/get expectation verification; no mocking framework"]
#[test]
fn test_ask() {
    todo!()
}

#[ignore = "requires a PHPUnit mock of QuestionHelper/HelperSet with ask/get expectation verification; no mocking framework"]
#[test]
fn test_ask_confirmation() {
    todo!()
}

#[ignore = "requires a PHPUnit mock of QuestionHelper/HelperSet with ask/get expectation verification; no mocking framework"]
#[test]
fn test_ask_and_validate() {
    todo!()
}

#[ignore = "requires a PHPUnit mock of QuestionHelper/HelperSet with ask/get expectation verification; no mocking framework"]
#[test]
fn test_select() {
    todo!()
}

#[test]
fn test_set_and_get_authentication() {
    let mut console_io = make_console_io();
    console_io.set_authentication(
        "repoName".to_string(),
        "l3l0".to_string(),
        Some("passwd".to_string()),
    );

    let mut expected: IndexMap<String, Option<String>> = IndexMap::new();
    expected.insert("username".to_string(), Some("l3l0".to_string()));
    expected.insert("password".to_string(), Some("passwd".to_string()));
    assert_eq!(expected, console_io.get_authentication("repoName"));
}

#[test]
fn test_get_authentication_when_did_not_set() {
    let console_io = make_console_io();

    let mut expected: IndexMap<String, Option<String>> = IndexMap::new();
    expected.insert("username".to_string(), None);
    expected.insert("password".to_string(), None);
    assert_eq!(expected, console_io.get_authentication("repoName"));
}

#[test]
fn test_has_authentication() {
    let mut console_io = make_console_io();
    console_io.set_authentication(
        "repoName".to_string(),
        "l3l0".to_string(),
        Some("passwd".to_string()),
    );

    assert!(console_io.has_authentication("repoName"));
    assert!(!console_io.has_authentication("repoName2"));
}

#[ignore = "data provider includes malformed-UTF-8 inputs (e.g. \\xFF, \\xC3\\x28); sanitize() takes PhpMixed::String which is UTF-8-only and cannot carry invalid bytes, so those cases are unrepresentable"]
#[test]
fn test_sanitize() {
    todo!()
}
