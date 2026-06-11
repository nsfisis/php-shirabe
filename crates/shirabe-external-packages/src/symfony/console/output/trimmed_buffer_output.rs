use crate::symfony::console::exception::InvalidArgumentException;
use crate::symfony::console::formatter::OutputFormatterInterface;
use crate::symfony::console::output::OutputInterface;
use crate::symfony::console::output::output::{DoWrite, Output};

/// A BufferedOutput that keeps only the last N chars.
#[derive(Debug)]
pub struct TrimmedBufferOutput {
    inner: Output,
    max_length: i64,
    buffer: std::cell::RefCell<String>,
}

impl TrimmedBufferOutput {
    pub fn new(
        max_length: i64,
        verbosity: Option<i64>,
        decorated: bool,
        formatter: Option<std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>>,
    ) -> Result<Self, InvalidArgumentException> {
        if max_length <= 0 {
            return Err(InvalidArgumentException(
                shirabe_php_shim::InvalidArgumentException {
                    message: shirabe_php_shim::sprintf(
                        "\"%s()\" expects a strictly positive maxLength. Got %d.",
                        &[
                            shirabe_php_shim::PhpMixed::String(
                                "Symfony\\Component\\Console\\Output\\TrimmedBufferOutput::__construct"
                                    .to_string(),
                            ),
                            shirabe_php_shim::PhpMixed::Int(max_length),
                        ],
                    ),
                    code: 0,
                },
            ));
        }

        Ok(Self {
            inner: Output::new(verbosity, decorated, formatter),
            max_length,
            buffer: std::cell::RefCell::new(String::new()),
        })
    }

    /// Empties buffer and returns its content.
    pub fn fetch(&self) -> String {
        let content = self.buffer.borrow().clone();
        *self.buffer.borrow_mut() = String::new();

        content
    }
}

impl DoWrite for TrimmedBufferOutput {
    fn do_write(&self, message: &str, newline: bool) {
        self.buffer.borrow_mut().push_str(message);

        if newline {
            self.buffer.borrow_mut().push_str(shirabe_php_shim::PHP_EOL);
        }

        let trimmed = shirabe_php_shim::substr(&self.buffer.borrow(), 0 - self.max_length, None);
        *self.buffer.borrow_mut() = trimmed;
    }
}

impl OutputInterface for TrimmedBufferOutput {
    fn write(&self, messages: &[String], newline: bool, options: i64) {
        self.inner.write(self, messages, newline, options);
    }
    fn writeln(&self, messages: &[String], options: i64) {
        self.inner.writeln(self, messages, options);
    }
    fn set_verbosity(&self, level: i64) {
        self.inner.set_verbosity(level);
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
    }
    fn is_decorated(&self) -> bool {
        self.inner.is_decorated()
    }
    fn set_formatter(
        &self,
        formatter: std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>,
    ) {
        self.inner.set_formatter(formatter);
    }
    fn get_formatter(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>> {
        self.inner.get_formatter()
    }
}
