use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct InputDefinition;

impl InputDefinition {
    pub fn new(_definition: Vec<PhpMixed>) -> Self {
        todo!()
    }

    pub fn add_argument(&mut self, _argument: PhpMixed) {
        todo!()
    }

    pub fn add_option(&mut self, _option: PhpMixed) {
        todo!()
    }

    pub fn has_option(&self, _name: &str) -> bool {
        todo!()
    }

    pub fn get_option(&self, _name: &str) -> anyhow::Result<PhpMixed> {
        todo!()
    }

    pub fn has_argument(&self, _name: &str) -> bool {
        todo!()
    }
}
