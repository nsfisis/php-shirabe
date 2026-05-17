use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct SymfonyStyle;

impl SymfonyStyle {
    pub fn new(_input: &dyn InputInterface, _output: &dyn OutputInterface) -> Self {
        todo!()
    }

    pub fn title(&mut self, _message: &str) {
        todo!()
    }

    pub fn section(&mut self, _message: &str) {
        todo!()
    }

    pub fn text(&mut self, _message: &str) {
        todo!()
    }

    pub fn comment(&mut self, _message: &str) {
        todo!()
    }

    pub fn success(&mut self, _message: PhpMixed) {
        todo!()
    }

    pub fn error(&mut self, _message: PhpMixed) {
        todo!()
    }

    pub fn warning(&mut self, _message: PhpMixed) {
        todo!()
    }

    pub fn note(&mut self, _message: PhpMixed) {
        todo!()
    }

    pub fn listing(&mut self, _elements: &[String]) {
        todo!()
    }

    pub fn new_line(&mut self, _count: i64) {
        todo!()
    }

    pub fn ask(
        &mut self,
        _question: &str,
        _default: Option<&str>,
        _validator: Option<Box<dyn Fn(Option<PhpMixed>) -> anyhow::Result<PhpMixed>>>,
    ) -> PhpMixed {
        todo!()
    }

    pub fn ask_hidden(
        &mut self,
        _question: &str,
        _validator: Option<Box<dyn Fn(Option<PhpMixed>) -> anyhow::Result<PhpMixed>>>,
    ) -> PhpMixed {
        todo!()
    }

    pub fn confirm(&mut self, _question: &str, _default: bool) -> bool {
        todo!()
    }

    pub fn choice(
        &mut self,
        _question: &str,
        _choices: Vec<PhpMixed>,
        _default: Option<PhpMixed>,
    ) -> PhpMixed {
        todo!()
    }

    pub fn table(&mut self, _headers: Vec<PhpMixed>, _rows: Vec<PhpMixed>) {
        todo!()
    }

    pub fn progress_start(&mut self, _max: i64) {
        todo!()
    }

    pub fn progress_advance(&mut self, _step: i64) {
        todo!()
    }

    pub fn progress_finish(&mut self) {
        todo!()
    }

    pub fn is_debug(&self) -> bool {
        todo!()
    }

    pub fn writeln(&mut self, _messages: PhpMixed, _type: i64) {
        todo!()
    }

    pub fn write(&mut self, _messages: PhpMixed, _newline: bool, _type: i64) {
        todo!()
    }
}
