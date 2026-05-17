use crate::symfony::component::console::question::question::Question;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct ChoiceQuestion(pub Question);

impl ChoiceQuestion {
    pub fn new(_question: &str, _choices: Vec<PhpMixed>, _default: Option<PhpMixed>) -> Self {
        todo!()
    }

    pub fn set_multiselect(&mut self, _multiselect: bool) {
        todo!()
    }

    pub fn set_error_message(&mut self, _error_message: &str) {
        todo!()
    }
}
