use crate::symfony::component::console::formatter::output_formatter::OutputFormatter;
use crate::symfony::component::console::output::output_interface::OutputInterface;

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

    pub fn get_error_output(&self) -> &dyn OutputInterface {
        todo!()
    }
}

impl OutputInterface for ConsoleOutput {
    fn write(&mut self, _messages: &str, _newline: bool, _type: i64) {
        todo!()
    }
    fn writeln(&mut self, _messages: &str, _type: i64) {
        todo!()
    }
    fn set_verbosity(&mut self, _level: i64) {
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
    fn set_decorated(&mut self, _decorated: bool) {
        todo!()
    }
    fn is_decorated(&self) -> bool {
        todo!()
    }
    fn set_formatter(&mut self, _formatter: OutputFormatter) {
        todo!()
    }
    fn get_formatter(&self) -> &OutputFormatter {
        todo!()
    }
}
