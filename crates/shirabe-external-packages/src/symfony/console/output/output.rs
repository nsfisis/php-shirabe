use crate::symfony::console::formatter::OutputFormatter;
use crate::symfony::console::formatter::OutputFormatterInterface;
use crate::symfony::console::output::output_interface::{
    OUTPUT_NORMAL, OUTPUT_PLAIN, OUTPUT_RAW, VERBOSITY_DEBUG, VERBOSITY_NORMAL, VERBOSITY_QUIET,
    VERBOSITY_VERBOSE, VERBOSITY_VERY_VERBOSE,
};

/// Base class for output classes.
///
/// There are five levels of verbosity:
///
///  * normal: no option passed (normal output)
///  * verbose: -v (more output)
///  * very verbose: -vv (highly extended output)
///  * debug: -vvv (all debug output)
///  * quiet: -q (no output)
pub struct Output {
    verbosity: std::cell::Cell<i64>,
    formatter: std::cell::RefCell<std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>>,
}

impl std::fmt::Debug for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Output")
            .field("verbosity", &self.verbosity)
            .finish_non_exhaustive()
    }
}

/// Subclasses provide the concrete sink by implementing `do_write`.
///
/// This mirrors the PHP `abstract protected function doWrite(string $message, bool $newline)`.
pub trait DoWrite {
    fn do_write(&self, message: &str, newline: bool);
}

impl Output {
    pub fn new(
        verbosity: Option<i64>,
        decorated: bool,
        formatter: Option<std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>>,
    ) -> Self {
        let verbosity = verbosity.unwrap_or(VERBOSITY_NORMAL);
        let formatter = formatter.unwrap_or_else(|| {
            std::rc::Rc::new(std::cell::RefCell::new(OutputFormatter::new(
                false,
                indexmap::IndexMap::new(),
            )))
        });
        formatter.borrow_mut().set_decorated(decorated);
        Self {
            verbosity: std::cell::Cell::new(verbosity),
            formatter: std::cell::RefCell::new(formatter),
        }
    }

    pub fn set_formatter(
        &self,
        formatter: std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>,
    ) {
        *self.formatter.borrow_mut() = formatter;
    }

    pub fn get_formatter(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>> {
        self.formatter.borrow().clone()
    }

    pub fn set_decorated(&self, decorated: bool) {
        self.formatter
            .borrow()
            .borrow_mut()
            .set_decorated(decorated);
    }

    pub fn is_decorated(&self) -> bool {
        self.formatter.borrow().borrow().is_decorated()
    }

    pub fn set_verbosity(&self, level: i64) {
        self.verbosity.set(level);
    }

    pub fn get_verbosity(&self) -> i64 {
        self.verbosity.get()
    }

    pub fn is_quiet(&self) -> bool {
        VERBOSITY_QUIET == self.verbosity.get()
    }

    pub fn is_verbose(&self) -> bool {
        VERBOSITY_VERBOSE <= self.verbosity.get()
    }

    pub fn is_very_verbose(&self) -> bool {
        VERBOSITY_VERY_VERBOSE <= self.verbosity.get()
    }

    pub fn is_debug(&self) -> bool {
        VERBOSITY_DEBUG <= self.verbosity.get()
    }

    pub fn writeln(&self, do_writer: &dyn DoWrite, messages: &[String], options: i64) {
        self.write(do_writer, messages, true, options);
    }

    pub fn write(&self, do_writer: &dyn DoWrite, messages: &[String], newline: bool, options: i64) {
        let types = OUTPUT_NORMAL | OUTPUT_RAW | OUTPUT_PLAIN;
        let r#type = {
            let masked = types & options;
            if masked != 0 { masked } else { OUTPUT_NORMAL }
        };

        let verbosities = VERBOSITY_QUIET
            | VERBOSITY_NORMAL
            | VERBOSITY_VERBOSE
            | VERBOSITY_VERY_VERBOSE
            | VERBOSITY_DEBUG;
        let verbosity = {
            let masked = verbosities & options;
            if masked != 0 {
                masked
            } else {
                VERBOSITY_NORMAL
            }
        };

        if verbosity > self.get_verbosity() {
            return;
        }

        for message in messages {
            let message = match r#type {
                OUTPUT_NORMAL => self
                    .formatter
                    .borrow()
                    .borrow_mut()
                    .format(Some(message))
                    .unwrap()
                    .unwrap_or_default(),
                OUTPUT_RAW => message.clone(),
                OUTPUT_PLAIN => shirabe_php_shim::strip_tags(
                    &self
                        .formatter
                        .borrow()
                        .borrow_mut()
                        .format(Some(message))
                        .unwrap()
                        .unwrap_or_default(),
                ),
                _ => message.clone(),
            };

            do_writer.do_write(&message, newline);
        }
    }
}
