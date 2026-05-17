use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct Question;

impl Question {
    pub fn new(_question: &str, _default: Option<PhpMixed>) -> Self {
        todo!()
    }

    pub fn set_validator(
        &mut self,
        _validator: Option<Box<dyn Fn(Option<PhpMixed>) -> anyhow::Result<PhpMixed>>>,
    ) {
        todo!()
    }

    pub fn set_max_attempts(&mut self, _attempts: Option<i64>) {
        todo!()
    }

    pub fn set_hidden(&mut self, _hidden: bool) {
        todo!()
    }

    pub fn set_hidden_fallback(&mut self, _fallback: bool) {
        todo!()
    }

    pub fn get_question(&self) -> String {
        todo!()
    }

    pub fn get_default(&self) -> Option<PhpMixed> {
        todo!()
    }

    pub fn is_hidden(&self) -> bool {
        todo!()
    }

    pub fn get_validator(&self) -> Option<&dyn Fn(Option<PhpMixed>) -> anyhow::Result<PhpMixed>> {
        todo!()
    }

    pub fn get_max_attempts(&self) -> Option<i64> {
        todo!()
    }
}
