use crate::symfony::component::console::input::input_definition::InputDefinition;
use crate::symfony::component::console::input::input_interface::InputInterface;
use crate::symfony::component::console::output::output_interface::OutputInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct Command;

impl Command {
    pub fn new(_name: Option<&str>) -> Self {
        todo!()
    }

    pub fn set_name(&mut self, _name: &str) -> &mut Self {
        todo!()
    }

    pub fn get_name(&self) -> Option<String> {
        todo!()
    }

    pub fn set_description(&mut self, _description: &str) -> &mut Self {
        todo!()
    }

    pub fn get_description(&self) -> String {
        todo!()
    }

    pub fn set_help(&mut self, _help: &str) -> &mut Self {
        todo!()
    }

    pub fn set_definition(&mut self, _definition: PhpMixed) -> &mut Self {
        todo!()
    }

    pub fn get_definition(&self) -> &InputDefinition {
        todo!()
    }

    pub fn add_argument(
        &mut self,
        _name: &str,
        _mode: Option<i64>,
        _description: &str,
        _default: PhpMixed,
    ) -> &mut Self {
        todo!()
    }

    pub fn add_option(
        &mut self,
        _name: &str,
        _shortcut: Option<&str>,
        _mode: Option<i64>,
        _description: &str,
        _default: PhpMixed,
    ) -> &mut Self {
        todo!()
    }

    pub fn set_aliases(&mut self, _aliases: &[String]) -> &mut Self {
        todo!()
    }

    pub fn get_aliases(&self) -> Vec<String> {
        todo!()
    }

    pub fn set_hidden(&mut self, _hidden: bool) -> &mut Self {
        todo!()
    }

    pub fn is_hidden(&self) -> bool {
        todo!()
    }

    pub fn run(
        &mut self,
        _input: &mut dyn InputInterface,
        _output: &mut dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        todo!()
    }

    pub fn get_helper(&self, _name: &str) -> PhpMixed {
        todo!()
    }

    pub fn get_helper_set(&self) -> PhpMixed {
        todo!()
    }
}
