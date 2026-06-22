//! ref: composer/vendor/symfony/console/Output/ConsoleOutput.php

use crate::symfony::console::formatter::OutputFormatterInterface;
use crate::symfony::console::output::ConsoleOutputInterface;
use crate::symfony::console::output::OutputInterface;
use crate::symfony::console::output::console_section_output::ConsoleSectionOutput;
use crate::symfony::console::output::output_interface::VERBOSITY_NORMAL;
use crate::symfony::console::output::stream_output::StreamOutput;

/// ConsoleOutput is the default class for all CLI output. It uses STDOUT and STDERR.
///
/// This class is a convenient wrapper around `StreamOutput` for both STDOUT and STDERR.
///
/// ```php
/// $output = new ConsoleOutput();
/// ```
///
/// This is equivalent to:
///
/// ```php
/// $output = new StreamOutput(fopen('php://stdout', 'w'));
/// $stdErr = new StreamOutput(fopen('php://stderr', 'w'));
/// ```
#[derive(Debug)]
pub struct ConsoleOutput {
    inner: StreamOutput,
    stderr: std::cell::RefCell<std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>>,
    console_section_outputs:
        std::rc::Rc<std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<ConsoleSectionOutput>>>>>,
}

impl ConsoleOutput {
    /// `$verbosity` defaults to `self::VERBOSITY_NORMAL`; pass `None` to use it.
    pub fn new(
        verbosity: Option<i64>,
        decorated: Option<bool>,
        formatter: Option<std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>>,
    ) -> anyhow::Result<Self> {
        let verbosity = verbosity.unwrap_or(VERBOSITY_NORMAL);

        let inner = StreamOutput::new(
            Self::open_output_stream(),
            Some(verbosity),
            decorated,
            formatter.clone(),
        )?
        .expect("ConsoleOutput stdout stream is always valid");

        let this = if formatter.is_none() {
            // for BC reasons, stdErr has it own Formatter only when user don't inject a specific formatter.
            let stderr =
                StreamOutput::new(Self::open_error_stream(), Some(verbosity), decorated, None)?
                    .expect("ConsoleOutput stderr stream is always valid");
            Self {
                inner,
                stderr: std::cell::RefCell::new(std::rc::Rc::new(std::cell::RefCell::new(stderr))),
                console_section_outputs: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            }
        } else {
            let actual_decorated = inner.is_decorated();
            let stderr = StreamOutput::new(
                Self::open_error_stream(),
                Some(verbosity),
                decorated,
                Some(inner.get_formatter()),
            )?
            .expect("ConsoleOutput stderr stream is always valid");
            let stderr_decorated = stderr.is_decorated();
            let this = Self {
                inner,
                stderr: std::cell::RefCell::new(std::rc::Rc::new(std::cell::RefCell::new(stderr))),
                console_section_outputs: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            };

            if decorated.is_none() {
                this.set_decorated(actual_decorated && stderr_decorated);
            }

            this
        };

        Ok(this)
    }

    /// Returns true if current environment supports writing console output to
    /// STDOUT.
    fn has_stdout_support() -> bool {
        !Self::is_running_os400()
    }

    /// Returns true if current environment supports writing console output to
    /// STDERR.
    fn has_stderr_support() -> bool {
        !Self::is_running_os400()
    }

    /// Checks if current executing environment is IBM iSeries (OS400), which
    /// doesn't properly convert character-encodings between ASCII to EBCDIC.
    fn is_running_os400() -> bool {
        let checks = [
            if shirabe_php_shim::function_exists("php_uname") {
                shirabe_php_shim::php_uname("s")
            } else {
                String::new()
            },
            shirabe_php_shim::getenv("OSTYPE").unwrap_or_default(),
            shirabe_php_shim::PHP_OS.to_string(),
        ];

        shirabe_php_shim::stripos(&shirabe_php_shim::implode(";", &checks), "OS400").is_some()
    }

    /// For testing only. Overwrites the inner `StreamOutput`'s private `stream` field, mirroring
    /// what `Symfony\Component\Console\Tester\TesterTrait::initOutput` does via reflection on the
    /// parent `StreamOutput::$stream` property of a `ConsoleOutput`.
    pub fn __set_stream(&mut self, stream: shirabe_php_shim::PhpResource) {
        self.inner.__set_stream(stream);
    }

    fn open_output_stream() -> shirabe_php_shim::PhpResource {
        if !Self::has_stdout_support() {
            return shirabe_php_shim::php_fopen_resource("php://output", "w");
        }

        // Use STDOUT when possible to prevent from opening too many file descriptors
        shirabe_php_shim::php_stdout_resource()
    }

    fn open_error_stream() -> shirabe_php_shim::PhpResource {
        if !Self::has_stderr_support() {
            return shirabe_php_shim::php_fopen_resource("php://output", "w");
        }

        // Use STDERR when possible to prevent from opening too many file descriptors
        shirabe_php_shim::php_stderr_resource()
    }
}

impl ConsoleOutputInterface for ConsoleOutput {
    /// Creates a new output section.
    fn section(&self) -> std::rc::Rc<std::cell::RefCell<ConsoleSectionOutput>> {
        // ConsoleSectionOutput::new pushes itself into the shared sections list.
        ConsoleSectionOutput::new(
            self.inner.get_stream().clone(),
            &self.console_section_outputs,
            self.get_verbosity(),
            self.is_decorated(),
            self.get_formatter(),
        )
    }

    fn get_error_output(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> {
        self.stderr.borrow().clone()
    }

    fn set_error_output(&self, error: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>) {
        *self.stderr.borrow_mut() = error;
    }
}

impl OutputInterface for ConsoleOutput {
    fn write(&self, messages: &[String], newline: bool, options: i64) {
        self.inner.write(messages, newline, options);
    }
    fn writeln(&self, messages: &[String], options: i64) {
        self.inner.writeln(messages, options);
    }
    fn set_verbosity(&self, level: i64) {
        self.inner.set_verbosity(level);
        self.stderr.borrow().borrow().set_verbosity(level);
    }
    fn get_verbosity(&self) -> i64 {
        self.inner.get_verbosity()
    }
    fn is_quiet(&self) -> bool {
        self.inner.is_quiet()
    }
    fn is_verbose(&self) -> bool {
        self.inner.is_verbose()
    }
    fn is_very_verbose(&self) -> bool {
        self.inner.is_very_verbose()
    }
    fn is_debug(&self) -> bool {
        self.inner.is_debug()
    }
    fn set_decorated(&self, decorated: bool) {
        self.inner.set_decorated(decorated);
        self.stderr.borrow().borrow().set_decorated(decorated);
    }
    fn is_decorated(&self) -> bool {
        self.inner.is_decorated()
    }
    fn set_formatter(
        &self,
        formatter: std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>,
    ) {
        self.inner.set_formatter(formatter.clone());
        self.stderr.borrow().borrow().set_formatter(formatter);
    }
    fn get_formatter(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>> {
        self.inner.get_formatter()
    }
}
