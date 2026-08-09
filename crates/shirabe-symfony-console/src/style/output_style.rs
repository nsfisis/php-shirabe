//! ref: composer/vendor/symfony/console/Style/OutputStyle.php

use crate::formatter::OutputFormatterInterface;
use crate::helper::ProgressBar;
use crate::output::ConsoleOutputInterface;
use crate::output::OutputInterface;
use crate::output::output_interface::OUTPUT_NORMAL;

/// Decorates output to add console style guide helpers.
#[derive(Debug)]
pub struct OutputStyle {
    output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
}

impl OutputStyle {
    pub fn new(output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>) -> Self {
        Self { output }
    }

    pub fn create_progress_bar(&self, max: i64) -> ProgressBar {
        ProgressBar::new(self.output.clone(), max, 1.0 / 25.0)
    }

    pub fn new_line(&self, count: i64) {
        self.output.borrow().write(
            &[shirabe_php_shim::str_repeat(
                shirabe_php_shim::PHP_EOL,
                count as usize,
            )],
            false,
            OUTPUT_NORMAL,
        );
    }

    pub(crate) fn get_error_output(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> {
        // PHP checks `$this->output instanceof ConsoleOutputInterface`; this requires
        // runtime type information that the OutputInterface trait object lacks.
        if !Self::is_console_output_interface(&self.output) {
            return self.output.clone();
        }

        Self::as_console_output_interface(&self.output)
            .unwrap()
            .get_error_output()
    }

    fn is_console_output_interface(
        output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> bool {
        // ConsoleOutput is the only OutputInterface implementor that also implements
        // ConsoleOutputInterface, so `instanceof ConsoleOutputInterface` reduces to this downcast.
        shirabe_php_shim::AsAny::as_any(&*output.borrow())
            .downcast_ref::<crate::output::console_output::ConsoleOutput>()
            .is_some()
    }

    /// PHP casts to `ConsoleOutputInterface`; `ConsoleOutput` being its only implementor, a
    /// borrow of the concrete type serves as the cast result.
    fn as_console_output_interface(
        output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> Option<std::cell::Ref<'_, crate::output::console_output::ConsoleOutput>> {
        std::cell::Ref::filter_map(output.borrow(), |output| {
            output
                .as_any()
                .downcast_ref::<crate::output::console_output::ConsoleOutput>()
        })
        .ok()
    }
}

impl OutputInterface for OutputStyle {
    fn write(&self, messages: &[String], newline: bool, options: i64) {
        self.output.borrow().write(messages, newline, options);
    }

    fn writeln(&self, messages: &[String], options: i64) {
        self.output.borrow().writeln(messages, options);
    }

    fn set_verbosity(&self, level: i64) {
        self.output.borrow().set_verbosity(level);
    }

    fn get_verbosity(&self) -> i64 {
        self.output.borrow().get_verbosity()
    }

    fn is_quiet(&self) -> bool {
        self.output.borrow().is_quiet()
    }

    fn is_verbose(&self) -> bool {
        self.output.borrow().is_verbose()
    }

    fn is_very_verbose(&self) -> bool {
        self.output.borrow().is_very_verbose()
    }

    fn is_debug(&self) -> bool {
        self.output.borrow().is_debug()
    }

    fn set_decorated(&self, decorated: bool) {
        self.output.borrow().set_decorated(decorated);
    }

    fn is_decorated(&self) -> bool {
        self.output.borrow().is_decorated()
    }

    fn set_formatter(
        &self,
        formatter: std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>,
    ) {
        self.output.borrow().set_formatter(formatter);
    }

    fn get_formatter(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>> {
        self.output.borrow().get_formatter()
    }
}
