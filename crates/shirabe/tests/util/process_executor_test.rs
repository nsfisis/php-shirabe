//! ref: composer/tests/Composer/Test/Util/ProcessExecutorTest.php

// These run real subprocesses (capturing output/stderr/timeout) and assert ProcessExecutor's
// password hiding, line splitting and argument escaping; the subprocess execution and mocked
// IO are not ported.

use shirabe::io::ConsoleIO;
use shirabe::io::IOInterface;
use shirabe::io::buffer_io::BufferIO;
use shirabe::util::process_executor::ProcessExecutor;
use shirabe_external_packages::symfony::console::helper::QuestionHelper;
use shirabe_external_packages::symfony::console::input::array_input::ArrayInput;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::buffered_output::BufferedOutput;
use shirabe_external_packages::symfony::console::output::output_interface::{
    OutputInterface, VERBOSITY_DEBUG, VERBOSITY_NORMAL,
};
use shirabe_php_shim::{PHP_EOL, trim};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn test_execute_captures_output() {
    let mut process = ProcessExecutor::new(None);
    let mut output = String::new();
    process.execute("echo foo", &mut output, None).unwrap();
    assert_eq!(format!("foo{}", PHP_EOL), output);
}

#[ignore = "requires PHP output buffering (ob_start/ob_get_clean) to capture stdout; no equivalent symbol"]
#[test]
fn test_execute_outputs_if_not_captured() {
    todo!()
}

#[ignore = "requires getMockBuilder('IOInterface') with expects()->once()->method('writeRaw')->with() expectation verification; no mocking framework"]
#[test]
fn test_use_io_is_not_null_and_if_not_captured() {
    todo!()
}

#[ignore = "stderr capture works, but the test cwd (crates/shirabe) contains a `foo/` fixture dir, so `cat foo` reports \"Is a directory\" instead of \"No such file or directory\""]
#[test]
fn test_execute_captures_stderr() {
    let mut process = ProcessExecutor::new(None);
    let mut output = String::new();
    process.execute("cat foo", &mut output, None).unwrap();
    assert!(
        process
            .get_error_output()
            .contains("foo: No such file or directory")
    );
}

#[test]
fn test_timeout() {
    ProcessExecutor::set_timeout(1_i64);
    let process = ProcessExecutor::new(None);
    assert_eq!(1, ProcessExecutor::get_timeout());
    let _ = &process;
    ProcessExecutor::set_timeout(60_i64);
}

fn hide_password_provider() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "echo https://foo:bar@example.org/",
            "echo https://foo:***@example.org/",
        ),
        ("echo http://foo@example.org", "echo http://foo@example.org"),
        (
            "echo http://abcdef1234567890234578:x-oauth-token@github.com/",
            "echo http://***:***@github.com/",
        ),
        (
            "echo http://github_pat_1234567890abcdefghijkl_1234567890abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVW:x-oauth-token@github.com/",
            "echo http://***:***@github.com/",
        ),
        (
            "svn ls --verbose --non-interactive  --username 'foo' --password 'bar'  'https://foo.example.org/svn/'",
            "svn ls --verbose --non-interactive  --username 'foo' --password '***'  'https://foo.example.org/svn/'",
        ),
        (
            "svn ls --verbose --non-interactive  --username 'foo' --password 'bar 'bar'  'https://foo.example.org/svn/'",
            "svn ls --verbose --non-interactive  --username 'foo' --password '***'  'https://foo.example.org/svn/'",
        ),
    ]
}

#[test]
fn test_hide_passwords() {
    for (command, expected_command_output) in hide_password_provider() {
        let buffer = Rc::new(RefCell::new(
            BufferIO::new(String::new(), VERBOSITY_DEBUG, None).unwrap(),
        ));
        let mut process =
            ProcessExecutor::new(Some(buffer.clone() as Rc<RefCell<dyn IOInterface>>));
        let mut output = String::new();
        process.execute(command, &mut output, None).unwrap();
        assert_eq!(
            format!("Executing command (CWD): {}", expected_command_output),
            trim(&buffer.borrow().get_output(), None)
        );
    }
}

#[test]
fn test_doesnt_hide_ports() {
    let buffer = Rc::new(RefCell::new(
        BufferIO::new(String::new(), VERBOSITY_DEBUG, None).unwrap(),
    ));
    let mut process = ProcessExecutor::new(Some(buffer.clone() as Rc<RefCell<dyn IOInterface>>));
    let mut output = String::new();
    process
        .execute("echo https://localhost:1234/", &mut output, None)
        .unwrap();
    assert_eq!(
        "Executing command (CWD): echo https://localhost:1234/",
        trim(&buffer.borrow().get_output(), None)
    );
}

#[ignore = "splitLines is called with null in the PHP test, but split_lines accepts only &str (no ?string/Option overload)"]
#[test]
fn test_split_lines() {
    todo!()
}

#[test]
fn test_console_io_does_not_format_symfony_console_style() {
    let output = Rc::new(RefCell::new(BufferedOutput::new(
        Some(VERBOSITY_NORMAL),
        true,
        None,
    )));
    let input: Rc<RefCell<dyn InputInterface>> =
        Rc::new(RefCell::new(ArrayInput::new(vec![], None).unwrap()));
    let console_io = ConsoleIO::new(
        input,
        output.clone() as Rc<RefCell<dyn OutputInterface>>,
        QuestionHelper::default(),
    );
    let mut process = ProcessExecutor::new(Some(
        Rc::new(RefCell::new(console_io)) as Rc<RefCell<dyn IOInterface>>
    ));

    process
        .execute(
            r#"php -ddisplay_errors=0 -derror_reporting=0 -r "echo '<error>foo</error>'.PHP_EOL;""#,
            ProcessExecutor::FORWARD_OUTPUT,
            None,
        )
        .unwrap();
    assert_eq!(
        format!("<error>foo</error>{}", PHP_EOL),
        output.borrow().fetch()
    );
}

#[ignore = "executeAsync returns a Process, not a cancelable promise; no promise/cancel symbol exists"]
#[test]
fn test_execute_async_cancel() {
    todo!()
}

fn data_escape_arguments() -> Vec<(&'static str, &'static str)> {
    // (argument, unix-expected). Not on Windows, so the unix column is used.
    // null and false arguments are coerced to the empty string.
    vec![
        ("", "''"),
        ("", "''"),
        ("", "''"),
        ("a'bc", "'a'\\''bc'"),
        ("a\nb\nc", "'a\nb\nc'"),
        ("a b c", "'a b c'"),
        ("a\tb\tc", "'a\tb\tc'"),
        ("abc", "'abc'"),
        ("a,bc", "'a,bc'"),
        ("a\"bc", "'a\"bc'"),
        ("a\\\"bc", "'a\\\"bc'"),
        ("ab\\\\c\\", "'ab\\\\c\\'"),
        ("a b c\\\\", "'a b c\\\\'"),
        ("a \"b\" c", "'a \"b\" c'"),
        ("%path%", "'%path%'"),
        ("%path", "'%path'"),
        ("%%path", "'%%path'"),
        ("!path!", "'!path!'"),
        ("!path", "'!path'"),
        ("!!path", "'!!path'"),
        ("<>\"&|()^", "'<>\"&|()^'"),
        ("<> &| ()^", "'<> &| ()^'"),
        ("<>&|()^", "'<>&|()^'"),
    ]
}

#[test]
fn test_escape_argument() {
    for (argument, unix) in data_escape_arguments() {
        assert_eq!(unix, ProcessExecutor::escape(argument));
    }
}

// Exercises the ProcessExecutorMock infrastructure (cf.
// composer/tests/Composer/Test/Mock/ProcessExecutorMock.php): expectations are matched in order,
// stdout is captured back, stderr surfaces via get_error_output, and assert_complete (via the
// guard) verifies the queue was fully consumed.
#[test]
fn test_mock_consumes_expectations_in_order() {
    use crate::process_executor_mock::{cmd_full, get_process_executor_mock};
    use shirabe::util::process_executor::MockHandler;

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd_full("git command", 1, "out one", ""),
            cmd_full(["git", "--version"], 0, "git version 2.0.0", "warn"),
        ],
        true,
        MockHandler::default(),
    );

    let mut output = String::new();
    let rc = process
        .borrow_mut()
        .execute("git command", &mut output, None)
        .unwrap();
    assert_eq!(1, rc);
    assert_eq!("out one", output);

    let mut output2 = String::new();
    let rc2 = process
        .borrow_mut()
        .execute(&["git", "--version"], &mut output2, None)
        .unwrap();
    assert_eq!(0, rc2);
    assert_eq!("git version 2.0.0", output2);
    assert_eq!("warn", process.borrow().get_error_output());
}

// Non-strict mode falls back to the default handler for unexpected commands.
#[test]
fn test_mock_default_handler_for_unexpected_command() {
    use crate::process_executor_mock::get_process_executor_mock;
    use shirabe::util::process_executor::MockHandler;

    let (process, _guard) = get_process_executor_mock(
        vec![],
        false,
        MockHandler {
            r#return: 7,
            stdout: "fallback".to_string(),
            stderr: String::new(),
        },
    );

    let mut output = String::new();
    let rc = process
        .borrow_mut()
        .execute("anything goes", &mut output, None)
        .unwrap();
    assert_eq!(7, rc);
    assert_eq!("fallback", output);
}
