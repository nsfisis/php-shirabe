use crate::symfony::component::console::input::input_interface::InputInterface;
use crate::symfony::component::console::output::output_interface::OutputInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct Application;

impl Application {
    pub fn new(_name: &str, _version: &str) -> Self {
        todo!()
    }

    pub fn run(
        &mut self,
        _input: Option<&mut dyn InputInterface>,
        _output: Option<&mut dyn OutputInterface>,
    ) -> anyhow::Result<i64> {
        todo!()
    }

    pub fn set_name(&mut self, _name: &str) {
        todo!()
    }

    pub fn get_name(&self) -> String {
        todo!()
    }

    pub fn set_version(&mut self, _version: &str) {
        todo!()
    }

    pub fn get_version(&self) -> String {
        todo!()
    }

    pub fn add(&mut self, _command: PhpMixed) -> Option<PhpMixed> {
        todo!()
    }

    pub fn get(&self, _name: &str) -> anyhow::Result<PhpMixed> {
        todo!()
    }

    pub fn set_auto_exit(&mut self, _auto_exit: bool) {
        todo!()
    }

    pub fn set_catch_exceptions(&mut self, _catch_exceptions: bool) {
        todo!()
    }

    pub fn get_helper_set(&self) -> PhpMixed {
        todo!()
    }

    pub fn set_helper_set(&mut self, _helper_set: PhpMixed) {
        todo!()
    }

    pub fn get_definition(&self) -> PhpMixed {
        todo!()
    }

    pub fn get_long_version(&self) -> String {
        todo!()
    }

    pub fn find(&self, _name: &str) -> anyhow::Result<PhpMixed> {
        todo!()
    }

    pub fn all(&self, _namespace: Option<&str>) -> Vec<PhpMixed> {
        todo!()
    }

    pub fn get_namespaces(&self) -> Vec<String> {
        todo!()
    }

    pub fn set_default_command(
        &mut self,
        _command_name: &str,
        _is_single_command: bool,
    ) -> &mut Self {
        todo!()
    }

    pub fn is_single_command(&self) -> bool {
        todo!()
    }
}
