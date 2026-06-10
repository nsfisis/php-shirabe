use crate::symfony::console::formatter::OutputFormatterStyle;

pub trait OutputFormatterInterface {
    fn is_decorated(&self) -> bool;
    fn set_decorated(&mut self, decorated: bool);
    fn set_style(&mut self, name: &str, style: OutputFormatterStyle);
    fn has_style(&self, name: &str) -> bool;
    fn get_style(&self, name: &str) -> OutputFormatterStyle;
    fn format(&self, message: &str) -> String;
}
