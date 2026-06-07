use crate::symfony::console::formatter::OutputFormatter;
use crate::symfony::console::output::ConsoleOutputInterface;
use crate::symfony::console::output::OutputInterface;

#[derive(Debug)]
pub struct ConsoleOutput;

impl ConsoleOutput {
    pub fn new(
        _verbosity: i64,
        _decorated: Option<bool>,
        _formatter: Option<OutputFormatter>,
    ) -> Self {
        todo!()
    }
}

impl ConsoleOutputInterface for ConsoleOutput {
    fn get_error_output(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> {
        todo!()
    }

    fn set_error_output(&mut self, _error: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>) {
        todo!()
    }
}

impl OutputInterface for ConsoleOutput {
    fn is_console_output_interface(&self) -> bool {
        true
    }

    fn as_console_output_interface(&self) -> Option<&dyn ConsoleOutputInterface> {
        Some(self)
    }

    fn write(&self, _messages: &str, _newline: bool, _type: i64) {
        todo!()
    }
    fn writeln(&self, _messages: &str, _type: i64) {
        todo!()
    }
    fn set_verbosity(&self, _level: i64) {
        todo!()
    }
    fn get_verbosity(&self) -> i64 {
        todo!()
    }
    fn is_quiet(&self) -> bool {
        todo!()
    }
    fn is_verbose(&self) -> bool {
        todo!()
    }
    fn is_very_verbose(&self) -> bool {
        todo!()
    }
    fn is_debug(&self) -> bool {
        todo!()
    }
    fn set_decorated(&self, _decorated: bool) {
        todo!()
    }
    fn is_decorated(&self) -> bool {
        todo!()
    }
    fn set_formatter(&self, _formatter: OutputFormatter) {
        todo!()
    }
    fn get_formatter(&self) -> &OutputFormatter {
        todo!()
    }
}
