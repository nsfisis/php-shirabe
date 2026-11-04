//! ref: composer/tests/Composer/Test/IO/ConsoleIOTest.php sha1:b1bbca3c0b7835dbab4922a1e9bc6b49e9f101f4

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

use bstr::{BStr, BString};
use indexmap::{IndexMap, indexmap};
use shirabe::io::io_interface::NORMAL;
use shirabe::io::{ConsoleIO, IOInterfaceImmutable, IOInterfaceMutable, IOInterfaceWrite};
use shirabe_php_shim::{
    PhpResource, bconcat, fwrite_resource, php_fopen_resource, rewind, str_repeat,
};
use shirabe_symfony_console::helper::QuestionHelper;
use shirabe_symfony_console::input::{
    ArrayInput, InputInterface, InputValue, StreamableInputInterface,
};
use shirabe_symfony_console::output::{
    BufferedOutput, ConsoleOutput, ConsoleOutputInterface, OutputInterface,
};

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
fn get_input_stream(input: &BStr) -> PhpResource {
    let stream = php_fopen_resource("php://memory", "r+");
    fwrite_resource(&stream, input);
    rewind(&stream);
    stream
}

/// Builds a ConsoleIO whose interactive input stream yields `answer`, plus a handle on the
/// `BufferedOutput` so the prompt can be inspected. The input is an `ArrayInput` (interactive by
/// default) with the answer stream attached, exactly as the question-helper tests set things up.
fn make_console_io_with_answer(
    answer: &BStr,
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

#[test]
fn test_write_error() {
    // PHP mocks ConsoleOutputInterface so getErrorOutput() returns the mock itself and expects
    // write('some information about something', false) once at VERBOSITY_NORMAL. A real
    // ConsoleOutput's error sink is stderr, which cannot be read back, but ConsoleOutputInterface
    // includes setErrorOutput; swapping in a BufferedOutput error sink lets the error-routed
    // write be read back, and writeError only ever reaches the sink returned by getErrorOutput.
    let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(ArrayInput::new(vec![], None).unwrap()),
    );
    let console_output = ConsoleOutput::new(None, Some(false), None).unwrap();
    let error_buffer = std::rc::Rc::new(std::cell::RefCell::new(BufferedOutput::new(
        None, false, None,
    )));
    console_output.set_error_output(error_buffer.clone());
    let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(console_output));
    let console_io = ConsoleIO::new(input, output, QuestionHelper::default());

    console_io.write_error3("some information about something", false, NORMAL);

    assert_eq!(
        "some information about something",
        error_buffer.borrow().fetch()
    );
}

#[ignore = "ConsoleIO::write3 takes a single string; the test feeds a 2-element array ['First line','Second lines'] and asserts a per-element regex on the debugging-prefixed messages array, which the string signature cannot represent"]
#[test]
fn test_write_with_multiple_line_string_when_debugging() {
    // TODO(type-model): ConsoleIO::write3 takes a single string; the test feeds a 2-element array
    // ['First line','Second lines'] and asserts a per-element regex on the debugging-prefixed
    // messages array, which the string signature cannot represent.
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

    let bs = |n: usize| str_repeat("\u{08}", n);
    let expected = bconcat!(
        "something (strlen = 23)",
        "\n",
        bs(23),
        "shorter (12)",
        str_repeat(" ", 11),
        bs(11),
        bs(12),
        "something longer than initial (34)",
        "\n"
    );
    assert_eq!(expected, buffered.borrow().fetch());
}

#[test]
fn test_ask() {
    // PHP asserts QuestionHelper::ask receives a Question. Behaviorally, an interactive input whose
    // stream yields the answer makes ConsoleIO::ask return that answer.
    let (console_io, _output) = make_console_io_with_answer(BStr::new("answer\n"));
    let result = console_io
        .ask("Why?", InputValue::String(BString::from("default")))
        .unwrap();
    assert_eq!(InputValue::String(BString::from("answer")), result);
}

#[test]
fn test_ask_confirmation() {
    // PHP asserts the helper receives a StrictConfirmationQuestion. Behaviorally, a "yes" answer
    // confirms and a "no" answer denies via the StrictConfirmationQuestion ConsoleIO builds.
    let (console_io, _output) = make_console_io_with_answer(BStr::new("yes\n"));
    assert!(console_io.ask_confirmation("Why?", false).unwrap());

    let (console_io, _output) = make_console_io_with_answer(BStr::new("no\n"));
    assert!(!console_io.ask_confirmation("Why?", false).unwrap());
}

#[test]
fn test_ask_and_validate() {
    // PHP asserts the helper receives a Question with a validator. Behaviorally, an always-true
    // validator passes the answer straight through.
    let (console_io, _output) = make_console_io_with_answer(BStr::new("answer\n"));
    let validator: Box<dyn Fn(InputValue) -> anyhow::Result<InputValue>> = Box::new(Ok);
    let result = console_io
        .ask_and_validate(
            "Why?",
            validator,
            Some(10),
            InputValue::String(BString::from("default")),
        )
        .unwrap();
    assert_eq!(InputValue::String(BString::from("answer")), result);
}

#[test]
fn test_select() {
    // PHP mocks the helper to return ['item2'] and asserts select maps it to ['1']. Behaviorally,
    // feeding "item2" to a multiselect ChoiceQuestion over the non-associative list
    // ["item1","item2"] resolves to that choice, which select maps back to its index '1'.
    let (console_io, _output) = make_console_io_with_answer(BStr::new("item2\n"));
    let result = console_io.select(
        "Select item",
        indexmap! {
            BString::from("0") => BString::from("item1"),
            BString::from("1") => BString::from("item2"),
        },
        InputValue::String(BString::from("item1")),
        None,
        "Error message",
        true,
    );
    assert_eq!(InputValue::Array(vec![BString::from("1")]), result.unwrap());
}

#[test]
fn test_set_and_get_authentication() {
    let mut console_io = make_console_io();
    console_io.set_authentication(
        bstr::BString::from("repoName"),
        bstr::BString::from("l3l0"),
        Some(bstr::BString::from("passwd")),
    );

    let mut expected: IndexMap<bstr::BString, Option<bstr::BString>> = IndexMap::new();
    expected.insert(BString::from("username"), Some(bstr::BString::from("l3l0")));
    expected.insert(
        BString::from("password"),
        Some(bstr::BString::from("passwd")),
    );
    assert_eq!(
        expected,
        console_io.get_authentication(bstr::BStr::new("repoName"))
    );
}

#[test]
fn test_get_authentication_when_did_not_set() {
    let console_io = make_console_io();

    let mut expected: IndexMap<bstr::BString, Option<bstr::BString>> = IndexMap::new();
    expected.insert(BString::from("username"), None);
    expected.insert(BString::from("password"), None);
    assert_eq!(
        expected,
        console_io.get_authentication(bstr::BStr::new("repoName"))
    );
}

#[test]
fn test_has_authentication() {
    let mut console_io = make_console_io();
    console_io.set_authentication(
        bstr::BString::from("repoName"),
        bstr::BString::from("l3l0"),
        Some(bstr::BString::from("passwd")),
    );

    assert!(console_io.has_authentication(bstr::BStr::new("repoName")));
    assert!(!console_io.has_authentication(bstr::BStr::new("repoName2")));
}

#[test]
fn test_sanitize() {
    // A PHP `string|string[]` input is sanitized element by element, so each case holds a list.
    let cases: Vec<(&BStr, Vec<&[u8]>, bool, Vec<&[u8]>)> = vec![
        (
            BStr::new("string with \\n allowed"),
            vec![b"Hello\nWorld".as_slice()],
            true,
            vec![b"Hello\nWorld".as_slice()],
        ),
        (
            BStr::new("string with \\r\\n allowed"),
            vec![b"Hello\r\nWorld".as_slice()],
            true,
            vec![b"Hello\r\nWorld".as_slice()],
        ),
        (
            BStr::new("string with standalone \\r removed"),
            vec![b"Hello\rWorld".as_slice()],
            true,
            vec![b"HelloWorld".as_slice()],
        ),
        (
            BStr::new("string with escape sequence removed"),
            vec![b"Hello\x1B[31mWorld".as_slice()],
            true,
            vec![b"HelloWorld".as_slice()],
        ),
        (
            BStr::new("string with control chars removed"),
            vec![b"Hello\x01\x08\x09World".as_slice()],
            true,
            vec![b"HelloWorld".as_slice()],
        ),
        (
            BStr::new("string with mixed control chars and newlines"),
            vec![b"Line1\n\x1B[32mLine2\x08\rLine3".as_slice()],
            true,
            vec![b"Line1\nLine2Line3".as_slice()],
        ),
        (
            BStr::new("string with null bytes are allowed"),
            vec![b"Hello\x00World".as_slice()],
            true,
            vec![b"Hello\x00World".as_slice()],
        ),
        (
            BStr::new("string with \\n removed"),
            vec![b"Hello\nWorld".as_slice()],
            false,
            vec![b"HelloWorld".as_slice()],
        ),
        (
            BStr::new("string with \\r\\n removed"),
            vec![b"Hello\r\nWorld".as_slice()],
            false,
            vec![b"HelloWorld".as_slice()],
        ),
        (
            BStr::new("string with escape sequence removed (no newlines)"),
            vec![b"Hello\x1B[31mWorld".as_slice()],
            false,
            vec![b"HelloWorld".as_slice()],
        ),
        (
            BStr::new("string with all control chars removed"),
            vec![b"Hello\x01\x08\x09\x0A\x0DWorld".as_slice()],
            false,
            vec![b"HelloWorld".as_slice()],
        ),
        (
            BStr::new("array with newlines allowed"),
            vec![b"Hello\nWorld".as_slice(), b"Foo\r\nBar".as_slice()],
            true,
            vec![b"Hello\nWorld".as_slice(), b"Foo\r\nBar".as_slice()],
        ),
        (
            BStr::new("array with control chars removed"),
            vec![b"Hello\x1B[31mWorld".as_slice(), b"Foo\x08Bar\r".as_slice()],
            true,
            vec![b"HelloWorld".as_slice(), b"FooBar".as_slice()],
        ),
        (
            BStr::new("array with newlines removed"),
            vec![b"Hello\nWorld".as_slice(), b"Foo\r\nBar".as_slice()],
            false,
            vec![b"HelloWorld".as_slice(), b"FooBar".as_slice()],
        ),
        (
            BStr::new("array with all control chars removed"),
            vec![b"Test\x01\x0A".as_slice(), b"Data\x1B[m\x0D".as_slice()],
            false,
            vec![b"Test".as_slice(), b"Data".as_slice()],
        ),
        (
            BStr::new("empty string"),
            vec![b"".as_slice()],
            true,
            vec![b"".as_slice()],
        ),
        (BStr::new("empty array"), vec![], true, vec![]),
        (
            BStr::new("string with no control chars"),
            vec![b"Hello World".as_slice()],
            true,
            vec![b"Hello World".as_slice()],
        ),
        (
            BStr::new("string with unicode"),
            vec![b"Hello \xE4\xB8\x96\xE7\x95\x8C\nTest".as_slice()],
            true,
            vec![b"Hello \xE4\xB8\x96\xE7\x95\x8C\nTest".as_slice()],
        ),
        (
            BStr::new("CSI with multiple parameters"),
            vec![b"Text\x1B[1;31;40mColored\x1B[0mNormal".as_slice()],
            true,
            vec![b"TextColoredNormal".as_slice()],
        ),
        (
            BStr::new("CSI SGR reset"),
            vec![b"Before\x1B[mAfter".as_slice()],
            true,
            vec![b"BeforeAfter".as_slice()],
        ),
        (
            BStr::new("CSI cursor positioning"),
            vec![b"Line\x1B[2J\x1B[H\x1B[10;5HText".as_slice()],
            true,
            vec![b"LineText".as_slice()],
        ),
        (
            BStr::new("OSC with BEL terminator"),
            vec![b"Text\x1B]0;Window Title\x07More".as_slice()],
            true,
            vec![b"TextMore".as_slice()],
        ),
        (
            BStr::new("OSC with ST terminator"),
            vec![b"Text\x1B]2;Title\x1B\\More".as_slice()],
            true,
            vec![b"TextMore".as_slice()],
        ),
        (
            BStr::new("Simple ESC sequences"),
            vec![b"Text\x1B7Saved\x1B8Restored\x1BcReset".as_slice()],
            true,
            vec![b"TextSavedRestoredReset".as_slice()],
        ),
        (
            BStr::new("ESC D (Index)"),
            vec![b"Line1\x1BDLine2".as_slice()],
            true,
            vec![b"Line1Line2".as_slice()],
        ),
        (
            BStr::new("ESC E (Next Line)"),
            vec![b"Line1\x1BELine2".as_slice()],
            true,
            vec![b"Line1Line2".as_slice()],
        ),
        (
            BStr::new("ESC M (Reverse Index)"),
            vec![b"Text\x1BMMore".as_slice()],
            true,
            vec![b"TextMore".as_slice()],
        ),
        (
            BStr::new("ESC N (SS2) and ESC O (SS3)"),
            vec![b"Text\x1BNchar\x1BOanother".as_slice()],
            true,
            vec![b"Textcharanother".as_slice()],
        ),
        (
            BStr::new("Multiple escape sequences in sequence"),
            vec![b"\x1B[1m\x1B[31m\x1B[44mBold Red on Blue\x1B[0m".as_slice()],
            true,
            vec![b"Bold Red on Blue".as_slice()],
        ),
        (
            BStr::new("CSI with question mark (private mode)"),
            vec![b"Text\x1B[?25lHidden\x1B[?25hVisible".as_slice()],
            true,
            vec![b"TextHiddenVisible".as_slice()],
        ),
        (
            BStr::new("CSI erase sequences"),
            vec![b"Clear\x1B[2J\x1B[K\x1B[1KScreen".as_slice()],
            true,
            vec![b"ClearScreen".as_slice()],
        ),
        (
            BStr::new("Hyperlink OSC 8"),
            vec![b"Click \x1B]8;;https://example.com\x1B\\here\x1B]8;;\x1B\\ for link".as_slice()],
            true,
            vec![b"Click here for link".as_slice()],
        ),
        (
            BStr::new("Mixed content with complex sequences"),
            vec![
                b"\x1B[1;33mWarning:\x1B[0m File\x1B[31m not\x1B[0m found\n\x1B[2KRetrying..."
                    .as_slice(),
            ],
            true,
            vec![b"Warning: File not found\nRetrying...".as_slice()],
        ),
        (
            BStr::new("malformed UTF-8 single byte"),
            vec![b"Hello\xFFWorld".as_slice()],
            true,
            vec![b"Hello?World".as_slice()],
        ),
        (
            BStr::new("malformed UTF-8 multiple bytes"),
            vec![b"Test\xC3\x28Data".as_slice()],
            true,
            vec![b"Test?(Data".as_slice()],
        ),
        (
            BStr::new("malformed UTF-8 with ANSI escape"),
            vec![b"Line\xFF\x1B[31mColor\xFE".as_slice()],
            true,
            vec![b"Line?Color?".as_slice()],
        ),
        (
            BStr::new("malformed UTF-8 in array"),
            vec![b"Item\xFF".as_slice(), b"Data\xC3\x28".as_slice()],
            true,
            vec![b"Item?".as_slice(), b"Data?(".as_slice()],
        ),
        (
            BStr::new("valid UTF-8 unchanged"),
            vec![b"Hello \xE4\xB8\x96\xE7\x95\x8C Test".as_slice()],
            true,
            vec![b"Hello \xE4\xB8\x96\xE7\x95\x8C Test".as_slice()],
        ),
        (
            BStr::new("mixed valid and invalid UTF-8"),
            vec![b"Hello\xFF\xE4\xB8\x96\xE7\x95\x8C\xFETest".as_slice()],
            true,
            vec![b"Hello?\xE4\xB8\x96\xE7\x95\x8C?Test".as_slice()],
        ),
    ];
    for (name, input, allow_newlines, expected) in cases {
        let actual: Vec<bstr::BString> = input
            .iter()
            .map(|message| ConsoleIO::sanitize(message, allow_newlines))
            .collect();
        let expected: Vec<bstr::BString> =
            expected.iter().map(|e| bstr::BString::from(*e)).collect();
        assert_eq!(expected, actual, "{}", name.display_lossy());
    }
}
