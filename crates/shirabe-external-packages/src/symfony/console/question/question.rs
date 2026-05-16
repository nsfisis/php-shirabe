use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct Question;

impl Question {
    pub fn new(question: &str, default: Option<PhpMixed>) -> Self {
        todo!()
    }

    pub fn set_validator(&mut self, validator: Option<Box<dyn Fn(Option<PhpMixed>) -> anyhow::Result<PhpMixed>>>) {
        todo!()
    }

    pub fn set_max_attempts(&mut self, attempts: Option<i64>) {
        todo!()
    }

    pub fn set_hidden(&mut self, hidden: bool) {
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
}
