#[derive(Debug)]
pub struct OutputFormatter;

impl OutputFormatter {
    pub fn new(
        _decorated: bool,
        _styles: indexmap::IndexMap<
            String,
            crate::symfony::console::formatter::OutputFormatterStyle,
        >,
    ) -> Self {
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
        _style: crate::symfony::console::formatter::OutputFormatterStyle,
    ) {
        todo!()
    }

    pub fn has_style(&self, _name: &str) -> bool {
        todo!()
    }

    pub fn get_style(
        &self,
        _name: &str,
    ) -> crate::symfony::console::formatter::OutputFormatterStyle {
        todo!()
    }
}

impl crate::symfony::console::formatter::OutputFormatterInterface for OutputFormatter {
    fn is_decorated(&self) -> bool {
        self.is_decorated()
    }

    fn set_decorated(&mut self, decorated: bool) {
        self.set_decorated(decorated)
    }

    fn set_style(
        &mut self,
        name: &str,
        style: crate::symfony::console::formatter::OutputFormatterStyle,
    ) {
        self.set_style(name, style)
    }

    fn has_style(&self, name: &str) -> bool {
        self.has_style(name)
    }

    fn get_style(&self, name: &str) -> crate::symfony::console::formatter::OutputFormatterStyle {
        self.get_style(name)
    }

    fn format(&self, message: &str) -> String {
        self.format(message)
    }
}
