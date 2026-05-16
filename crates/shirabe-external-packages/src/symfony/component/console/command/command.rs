use shirabe_php_shim::PhpMixed;
use crate::symfony::component::console::input::input_interface::InputInterface;
use crate::symfony::component::console::output::output_interface::OutputInterface;
use crate::symfony::component::console::input::input_definition::InputDefinition;

#[derive(Debug)]
pub struct Command;

impl Command {
    pub fn new(name: Option<&str>) -> Self {
        todo!()
    }

    pub fn set_name(&mut self, name: &str) -> &mut Self {
        todo!()
    }

    pub fn get_name(&self) -> Option<String> {
        todo!()
    }

    pub fn set_description(&mut self, description: &str) -> &mut Self {
        todo!()
    }

    pub fn get_description(&self) -> String {
        todo!()
    }

    pub fn set_help(&mut self, help: &str) -> &mut Self {
        todo!()
    }

    pub fn set_definition(&mut self, definition: PhpMixed) -> &mut Self {
        todo!()
    }

    pub fn get_definition(&self) -> &InputDefinition {
        todo!()
    }

    pub fn add_argument(&mut self, name: &str, mode: Option<i64>, description: &str, default: PhpMixed) -> &mut Self {
        todo!()
    }

    pub fn add_option(&mut self, name: &str, shortcut: Option<&str>, mode: Option<i64>, description: &str, default: PhpMixed) -> &mut Self {
        todo!()
    }

    pub fn set_aliases(&mut self, aliases: &[String]) -> &mut Self {
        todo!()
    }

    pub fn get_aliases(&self) -> Vec<String> {
        todo!()
    }

    pub fn set_hidden(&mut self, hidden: bool) -> &mut Self {
        todo!()
    }

    pub fn is_hidden(&self) -> bool {
        todo!()
    }

    pub fn run(
        &mut self,
        input: &mut dyn InputInterface,
        output: &mut dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        todo!()
    }

    pub fn get_helper(&self, name: &str) -> PhpMixed {
        todo!()
    }

    pub fn get_helper_set(&self) -> PhpMixed {
        todo!()
    }
}
