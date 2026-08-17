//! ref: composer/tests/Composer/Test/Mock/ProcessExecutorMock.php

use shirabe::util::process_executor::{CommandLine, MockExpectation, MockHandler, ProcessExecutor};

// A command expectation as written in the PHP tests: either a bare command
// (`'git command'` / `['git', '--version']`) or the full
// `{cmd, return?, stdout?, stderr?}` associative form.
pub fn cmd(command: impl IntoMockCmd) -> MockExpectation {
    MockExpectation::from_cmd(command.into_mock_cmd())
}

pub fn cmd_full(
    command: impl IntoMockCmd,
    r#return: i64,
    stdout: impl Into<String>,
    stderr: impl Into<String>,
) -> MockExpectation {
    MockExpectation {
        cmd: command.into_mock_cmd(),
        r#return,
        stdout: stdout.into(),
        stderr: stderr.into(),
        callback: None,
    }
}

// Accepts the same command shapes the tests use: a string command or a list of
// string args. Comparison against the executed command is exact (PHP `===`), so
// the form here must match the form the code under test passes to `execute`.
pub trait IntoMockCmd {
    fn into_mock_cmd(self) -> CommandLine;
}

impl IntoMockCmd for &str {
    fn into_mock_cmd(self) -> CommandLine {
        CommandLine::Shell(self.to_string())
    }
}

impl IntoMockCmd for String {
    fn into_mock_cmd(self) -> CommandLine {
        CommandLine::Shell(self)
    }
}

impl IntoMockCmd for Vec<&str> {
    fn into_mock_cmd(self) -> CommandLine {
        CommandLine::Args(self.into_iter().map(|s| s.to_string()).collect())
    }
}

impl IntoMockCmd for Vec<String> {
    fn into_mock_cmd(self) -> CommandLine {
        CommandLine::Args(self)
    }
}

impl<const N: usize> IntoMockCmd for [&str; N] {
    fn into_mock_cmd(self) -> CommandLine {
        CommandLine::Args(self.iter().map(|s| s.to_string()).collect())
    }
}

pub struct ProcessExecutorMockGuard(std::rc::Rc<std::cell::RefCell<ProcessExecutor>>);

impl Drop for ProcessExecutorMockGuard {
    fn drop(&mut self) {
        // Avoid aborting on a double panic when a test assertion is already unwinding.
        if std::thread::panicking() {
            return;
        }
        self.0.borrow().__assert_complete();
    }
}

// For testing only. Mirrors TestCase::getProcessExecutorMock: returns a shared
// ProcessExecutor handle configured with the given expectations, plus a guard
// that runs `__assert_complete` when it drops at the end of the test scope.
pub fn get_process_executor_mock(
    expectations: Vec<MockExpectation>,
    strict: bool,
    default_handler: MockHandler,
) -> (
    std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ProcessExecutorMockGuard,
) {
    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)));
    process
        .borrow_mut()
        .__expects(expectations, strict, default_handler);
    (process.clone(), ProcessExecutorMockGuard(process))
}
