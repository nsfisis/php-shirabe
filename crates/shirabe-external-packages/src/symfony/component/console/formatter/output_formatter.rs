#[derive(Debug)]
pub struct OutputFormatter;

impl OutputFormatter {
    pub fn new(decorated: bool) -> Self {
        todo!()
    }

    pub fn format(&self, message: &str) -> String {
        todo!()
    }

    pub fn is_decorated(&self) -> bool {
        todo!()
    }

    pub fn set_decorated(&mut self, decorated: bool) {
        todo!()
    }

    pub fn escape(text: &str) -> String {
        todo!()
    }

    pub fn escape_trailing_backslash(text: &str) -> String {
        todo!()
    }

    pub fn set_style(&mut self, name: &str, style: crate::symfony::component::console::formatter::output_formatter_style::OutputFormatterStyle) {
        todo!()
    }
}
