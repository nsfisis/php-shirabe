use crate::symfony::console::input::InputArgument;
use crate::symfony::console::input::InputOption;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct InputDefinition;

impl InputDefinition {
    pub fn new(_definition: Vec<PhpMixed>) -> Self {
        todo!()
    }

    pub fn add_argument(&mut self, _argument: InputArgument) {
        todo!()
    }

    pub fn add_option(&mut self, _option: InputOption) {
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
