//! ref: composer/tests/Composer/Test/IO/ConsoleIOTest.php

// PHP drives ConsoleIO with PHPUnit mocks of Symfony's InputInterface/OutputInterface/HelperSet
// (and QuestionHelper). There is no mocking framework here, so the mocks are reproduced with real,
// side-effect-free Symfony console types:
//   * `expects()->method('write')->with(...)` call/argument expectations are replaced by reading
//     back what a real `BufferedOutput` collected and asserting on it.
//   * `isInteractive` consecutive return values are reproduced by toggling a real `ArrayInput`'s
//     interactive flag between calls.
//   * QuestionHelper `ask`/`get` expectations are replaced by feeding a prepared answer through a
//     `php://memory` input stream (exactly as StrictConfirmationQuestionTest does) and asserting on
//     the returned value.

use indexmap::IndexMap;
use shirabe::io::ConsoleIO;
use shirabe::io::IOInterfaceImmutable;
use shirabe::io::IOInterfaceMutable;
use shirabe::io::io_interface::NORMAL;
use shirabe_external_packages::symfony::console::helper::QuestionHelper;
use shirabe_external_packages::symfony::console::input::array_input::ArrayInput;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::input::streamable_input_interface::StreamableInputInterface;
use shirabe_external_packages::symfony::console::output::buffered_output::BufferedOutput;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::PhpMixed;

/// Builds a ConsoleIO backed by real, side-effect-free Input/Output/QuestionHelper. PHP uses
/// PHPUnit mocks here, but for tests that exercise only the authentication map they carry no
/// expectations, so concrete implementations are an exact substitute.
fn make_console_io() -> ConsoleIO {
    let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(ArrayInput::new(vec![], None).unwrap()),
    );
    let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(BufferedOutput::new(None, false, None)),
    );
    ConsoleIO::new(input, output, QuestionHelper::default())
}

/// Opens a `php://memory` stream pre-filled with `input`, mirroring
/// StrictConfirmationQuestionTest::getInputStream.
fn get_input_stream(input: &str) -> shirabe_php_shim::PhpResource {
    let stream = shirabe_php_shim::php_fopen_resource("php://memory", "r+");
    shirabe_php_shim::fwrite_resource(&stream, input);
    shirabe_php_shim::rewind(&stream);
    stream
}

/// Builds a ConsoleIO whose interactive input stream yields `answer`, plus a handle on the
/// `BufferedOutput` so the prompt can be inspected. The input is an `ArrayInput` (interactive by
/// default) with the answer stream attached, exactly as the question-helper tests set things up.
fn make_console_io_with_answer(
    answer: &str,
) -> (ConsoleIO, std::rc::Rc<std::cell::RefCell<BufferedOutput>>) {
    let mut array_input = ArrayInput::new(vec![], None).unwrap();
    array_input.set_stream(get_input_stream(answer));
    let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(array_input));
    let buffered = std::rc::Rc::new(std::cell::RefCell::new(BufferedOutput::new(
        None, false, None,
    )));
    let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = buffered.clone();
    (
        ConsoleIO::new(input, output, QuestionHelper::default()),
        buffered,
    )
}

#[test]
fn test_is_interactive() {
    // PHP mocks isInteractive to return true then false on consecutive calls. A real ArrayInput
    // exposes a mutable interactive flag, so toggling it between the two asserts is equivalent.
    let array_input = ArrayInput::new(vec![], None).unwrap();
    let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(array_input));
    let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(BufferedOutput::new(None, false, None)),
    );
    let console_io = ConsoleIO::new(input.clone(), output, QuestionHelper::default());

    input.borrow_mut().set_interactive(true);
    assert!(console_io.is_interactive());
    input.borrow_mut().set_interactive(false);
    assert!(!console_io.is_interactive());
}

#[test]
fn test_write() {
    // PHP: expects write('some information about something', false) at VERBOSITY_NORMAL.
    // A plain (non-Console) OutputInterface routes write to the main output, so the message lands
    // in the BufferedOutput verbatim (no trailing EOL since newline is false).
    let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(ArrayInput::new(vec![], None).unwrap()),
    );
    let buffered = std::rc::Rc::new(std::cell::RefCell::new(BufferedOutput::new(
        None, false, None,
    )));
    let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = buffered.clone();
    let console_io = ConsoleIO::new(input, output, QuestionHelper::default());

    console_io.write3("some information about something", false, NORMAL);

    assert_eq!(
        "some information about something",
        buffered.borrow().fetch()
    );
}

#[ignore = "PHP mocks ConsoleOutputInterface so getErrorOutput returns the same mock; a real ConsoleOutput's error StreamOutput writes to php://stderr, which cannot be read back, and the trait offers no seam to inject a BufferedOutput error sink"]
#[test]
fn test_write_error() {
    // TODO(phase-d): PHP mocks ConsoleOutputInterface so getErrorOutput returns the same mock; a
    // real ConsoleOutput's error StreamOutput writes to php://stderr, which cannot be read back,
    // and the trait offers no seam to inject a BufferedOutput error sink.
    todo!()
}

#[ignore = "ConsoleIO::write3 takes a single &str; the test feeds a 2-element array ['First line','Second lines'] and asserts a per-element regex on the debugging-prefixed messages array, which the &str signature cannot represent"]
#[test]
fn test_write_with_multiple_line_string_when_debugging() {
    // TODO(phase-d): ConsoleIO::write3 takes a single &str; the test feeds a 2-element array
    // ['First line','Second lines'] and asserts a per-element regex on the debugging-prefixed
    // messages array, which the &str signature cannot represent.
    todo!()
}

#[test]
fn test_overwrite() {
    // PHP asserts the exact series of write() calls overwrite produces (backspaces, padding, etc.).
    // PHP's mock intercepts each raw write() argument, before the output formatter runs. A real
    // BufferedOutput captures the *rendered* text, so the `<question>/<comment>/<info>` tags are
    // stripped while the backspace/space runs are identical. The backspace counts still derive from
    // `strlen(strip_tags(lastMessage))`, so they match PHP exactly. The series, in order, is:
    //   'something (strlen = 23)' + "\n"   (initial write, newline defaults true)
    //   "\x08" * 23                        (clear previous, strip_tags len = 23)
    //   'shorter (12)'                     (new message, strip_tags len = 12)
    //   " " * 11                           (fill: 23 - 12)
    //   "\x08" * 11                        (move cursor back)
    //   "\x08" * 12                        (clear previous, len = 12)
    //   'something longer than initial (34)' (new message)
    // The final overwrite needs no fill and newline defaults true, appending a trailing "\n".
    let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(ArrayInput::new(vec![], None).unwrap()),
    );
    let buffered = std::rc::Rc::new(std::cell::RefCell::new(BufferedOutput::new(
        None, false, None,
    )));
    let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = buffered.clone();
    let console_io = ConsoleIO::new(input, output, QuestionHelper::default());

    console_io.write3("something (<question>strlen = 23</question>)", true, NORMAL);
    console_io.overwrite4("shorter (<comment>12</comment>)", false, None, NORMAL);
    console_io.overwrite4(
        "something longer than initial (<info>34</info>)",
        true,
        None,
        NORMAL,
    );

    let bs = |n: usize| "\u{08}".repeat(n);
    let expected = format!(
        "{}\n{}{}{}{}{}{}\n",
        "something (strlen = 23)",
        bs(23),
        "shorter (12)",
        " ".repeat(11),
        bs(11),
        bs(12),
        "something longer than initial (34)",
    );
    assert_eq!(expected, buffered.borrow().fetch());
}

#[test]
fn test_ask() {
    // PHP asserts QuestionHelper::ask receives a Question. Behaviorally, an interactive input whose
    // stream yields the answer makes ConsoleIO::ask return that answer.
    let (console_io, _output) = make_console_io_with_answer("answer\n");
    let result = console_io.ask("Why?".to_string(), PhpMixed::String("default".to_string()));
    assert_eq!(PhpMixed::String("answer".to_string()), result);
}

#[test]
fn test_ask_confirmation() {
    // PHP asserts the helper receives a StrictConfirmationQuestion. Behaviorally, a "yes" answer
    // confirms and a "no" answer denies via the StrictConfirmationQuestion ConsoleIO builds.
    let (console_io, _output) = make_console_io_with_answer("yes\n");
    assert!(console_io.ask_confirmation("Why?".to_string(), false));

    let (console_io, _output) = make_console_io_with_answer("no\n");
    assert!(!console_io.ask_confirmation("Why?".to_string(), false));
}

#[test]
fn test_ask_and_validate() {
    // PHP asserts the helper receives a Question with a validator. Behaviorally, an always-true
    // validator passes the answer straight through.
    let (console_io, _output) = make_console_io_with_answer("answer\n");
    let validator: Box<dyn Fn(PhpMixed) -> anyhow::Result<PhpMixed>> = Box::new(Ok);
    let result = console_io
        .ask_and_validate(
            "Why?".to_string(),
            validator,
            Some(10),
            PhpMixed::String("default".to_string()),
        )
        .unwrap();
    assert_eq!(PhpMixed::String("answer".to_string()), result);
}

#[test]
fn test_select() {
    // PHP mocks the helper to return ['item2'] and asserts select maps it to ['1']. Behaviorally,
    // feeding "item2" to a multiselect ChoiceQuestion over the non-associative list
    // ["item1","item2"] resolves to that choice, which select maps back to its index '1'.
    let (console_io, _output) = make_console_io_with_answer("item2\n");
    let result = console_io.select(
        "Select item".to_string(),
        vec!["item1".to_string(), "item2".to_string()],
        PhpMixed::String("item1".to_string()),
        PhpMixed::Bool(false),
        "Error message".to_string(),
        true,
    );
    assert_eq!(
        PhpMixed::List(vec![PhpMixed::String("1".to_string())]),
        result
    );
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
    // TODO(phase-d): the data provider includes malformed-UTF-8 inputs (e.g. \xFF, \xC3\x28);
    // sanitize() takes PhpMixed::String which is UTF-8-only and cannot carry invalid bytes, so
    // those cases are unrepresentable.
    todo!()
}
