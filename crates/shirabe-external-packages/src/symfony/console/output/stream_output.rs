use crate::symfony::console::formatter::OutputFormatterInterface;
use crate::symfony::console::output::OutputInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct StreamOutput;

impl StreamOutput {
    pub fn new(_stream: PhpMixed, _verbosity: i64, _decorated: Option<bool>) -> Self {
        todo!()
    }
}

impl OutputInterface for StreamOutput {
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
    fn set_formatter(
        &self,
        _formatter: std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>,
    ) {
        todo!()
    }
    fn get_formatter(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>> {
        todo!()
    }
    fn get_stream(&self) -> shirabe_php_shim::PhpResource {
        todo!()
    }
}
