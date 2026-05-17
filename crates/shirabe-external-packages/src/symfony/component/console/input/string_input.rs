use crate::symfony::component::console::input::input_definition::InputDefinition;
use crate::symfony::component::console::input::input_interface::InputInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct StringInput;

impl StringInput {
    pub fn new(_input: &str) -> Self {
        todo!()
    }
}

impl InputInterface for StringInput {
    fn get_first_argument(&self) -> Option<String> {
        todo!()
    }
    fn has_parameter_option(&self, _values: &[&str], _only_params: bool) -> bool {
        todo!()
    }
    fn get_parameter_option(
        &self,
        _values: &[&str],
        _default: PhpMixed,
        _only_params: bool,
    ) -> PhpMixed {
        todo!()
    }
    fn bind(&mut self, _definition: &InputDefinition) -> anyhow::Result<()> {
        todo!()
    }
    fn validate(&self) -> anyhow::Result<()> {
        todo!()
    }
    fn get_arguments(&self) -> IndexMap<String, PhpMixed> {
        todo!()
    }
    fn get_argument(&self, _name: &str) -> PhpMixed {
        todo!()
    }
    fn set_argument(&mut self, _name: &str, _value: PhpMixed) -> anyhow::Result<()> {
        todo!()
    }
    fn has_argument(&self, _name: &str) -> bool {
        todo!()
    }
    fn get_options(&self) -> IndexMap<String, PhpMixed> {
        todo!()
    }
    fn get_option(&self, _name: &str) -> PhpMixed {
        todo!()
    }
    fn set_option(&mut self, _name: &str, _value: PhpMixed) -> anyhow::Result<()> {
        todo!()
    }
    fn has_option(&self, _name: &str) -> bool {
        todo!()
    }
    fn is_interactive(&self) -> bool {
        todo!()
    }
    fn set_interactive(&mut self, _interactive: bool) {
        todo!()
    }
}
