use crate::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct StreamOutput;

impl StreamOutput {
    pub fn new(_stream: PhpMixed, _verbosity: i64, _decorated: Option<bool>) -> Self {
        todo!()
    }
}

impl OutputInterface for StreamOutput {
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
}
