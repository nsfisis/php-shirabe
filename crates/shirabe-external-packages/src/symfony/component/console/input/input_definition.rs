use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct InputDefinition;

impl InputDefinition {
    pub fn new(definition: Vec<PhpMixed>) -> Self {
        todo!()
    }

    pub fn add_argument(&mut self, argument: PhpMixed) {
        todo!()
    }

    pub fn add_option(&mut self, option: PhpMixed) {
        todo!()
    }

    pub fn has_option(&self, name: &str) -> bool {
        todo!()
    }

    pub fn get_option(&self, name: &str) -> anyhow::Result<PhpMixed> {
        todo!()
    }

    pub fn has_argument(&self, name: &str) -> bool {
        todo!()
    }
}
