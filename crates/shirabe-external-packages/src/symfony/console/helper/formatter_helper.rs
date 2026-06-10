use crate::symfony::console::helper::HelperInterface;

#[derive(Debug)]
pub struct FormatterHelper;

impl FormatterHelper {
    pub fn format_section(&self, _section: &str, _message: &str, _style: &str) -> String {
        todo!()
    }

    pub fn format_block(&self, _messages: &[&str], _style: &str, _large: bool) -> String {
        todo!()
    }
}

impl HelperInterface for FormatterHelper {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
