use crate::symfony::component::console::question::question::Question;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct ChoiceQuestion(pub Question);

impl ChoiceQuestion {
    pub fn new(question: &str, choices: Vec<PhpMixed>, default: Option<PhpMixed>) -> Self {
        todo!()
    }

    pub fn set_multiselect(&mut self, multiselect: bool) {
        todo!()
    }

    pub fn set_error_message(&mut self, error_message: &str) {
        todo!()
    }
}
