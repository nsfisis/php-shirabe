use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct SymfonyStyle;

impl SymfonyStyle {
    pub fn new(input: &dyn InputInterface, output: &dyn OutputInterface) -> Self {
        todo!()
    }

    pub fn title(&mut self, message: &str) {
        todo!()
    }

    pub fn section(&mut self, message: &str) {
        todo!()
    }

    pub fn text(&mut self, message: &str) {
        todo!()
    }

    pub fn comment(&mut self, message: &str) {
        todo!()
    }

    pub fn success(&mut self, message: PhpMixed) {
        todo!()
    }

    pub fn error(&mut self, message: PhpMixed) {
        todo!()
    }

    pub fn warning(&mut self, message: PhpMixed) {
        todo!()
    }

    pub fn note(&mut self, message: PhpMixed) {
        todo!()
    }

    pub fn listing(&mut self, elements: &[String]) {
        todo!()
    }

    pub fn new_line(&mut self, count: i64) {
        todo!()
    }

    pub fn ask(
        &mut self,
        question: &str,
        default: Option<&str>,
        validator: Option<Box<dyn Fn(Option<PhpMixed>) -> anyhow::Result<PhpMixed>>>,
    ) -> PhpMixed {
        todo!()
    }

    pub fn ask_hidden(
        &mut self,
        question: &str,
        validator: Option<Box<dyn Fn(Option<PhpMixed>) -> anyhow::Result<PhpMixed>>>,
    ) -> PhpMixed {
        todo!()
    }

    pub fn confirm(&mut self, question: &str, default: bool) -> bool {
        todo!()
    }

    pub fn choice(
        &mut self,
        question: &str,
        choices: Vec<PhpMixed>,
        default: Option<PhpMixed>,
    ) -> PhpMixed {
        todo!()
    }

    pub fn table(&mut self, headers: Vec<PhpMixed>, rows: Vec<PhpMixed>) {
        todo!()
    }

    pub fn progress_start(&mut self, max: i64) {
        todo!()
    }

    pub fn progress_advance(&mut self, step: i64) {
        todo!()
    }

    pub fn progress_finish(&mut self) {
        todo!()
    }

    pub fn is_debug(&self) -> bool {
        todo!()
    }

    pub fn writeln(&mut self, messages: PhpMixed, r#type: i64) {
        todo!()
    }

    pub fn write(&mut self, messages: PhpMixed, newline: bool, r#type: i64) {
        todo!()
    }
}
