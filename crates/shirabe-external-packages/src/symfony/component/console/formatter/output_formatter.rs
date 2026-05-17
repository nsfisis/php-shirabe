#[derive(Debug)]
pub struct OutputFormatter;

impl OutputFormatter {
    pub fn new(_decorated: bool) -> Self {
        todo!()
    }

    pub fn format(&self, _message: &str) -> String {
        todo!()
    }

    pub fn is_decorated(&self) -> bool {
        todo!()
    }

    pub fn set_decorated(&mut self, _decorated: bool) {
        todo!()
    }

    pub fn escape(_text: &str) -> String {
        todo!()
    }

    pub fn escape_trailing_backslash(_text: &str) -> String {
        todo!()
    }

    pub fn set_style(
        &mut self,
        _name: &str,
        _style: crate::symfony::component::console::formatter::output_formatter_style::OutputFormatterStyle,
    ) {
        todo!()
    }
}
