use crate::symfony::component::console::input::input_interface::InputInterface;
use crate::symfony::component::console::output::output_interface::OutputInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct Application;

impl Application {
    pub fn new(name: &str, version: &str) -> Self {
        todo!()
    }

    pub fn run(
        &mut self,
        input: Option<&mut dyn InputInterface>,
        output: Option<&mut dyn OutputInterface>,
    ) -> anyhow::Result<i64> {
        todo!()
    }

    pub fn set_name(&mut self, name: &str) {
        todo!()
    }

    pub fn get_name(&self) -> String {
        todo!()
    }

    pub fn set_version(&mut self, version: &str) {
        todo!()
    }

    pub fn get_version(&self) -> String {
        todo!()
    }

    pub fn add(&mut self, command: PhpMixed) -> Option<PhpMixed> {
        todo!()
    }

    pub fn get(&self, name: &str) -> anyhow::Result<PhpMixed> {
        todo!()
    }

    pub fn set_auto_exit(&mut self, auto_exit: bool) {
        todo!()
    }

    pub fn set_catch_exceptions(&mut self, catch_exceptions: bool) {
        todo!()
    }

    pub fn get_helper_set(&self) -> PhpMixed {
        todo!()
    }

    pub fn set_helper_set(&mut self, helper_set: PhpMixed) {
        todo!()
    }

    pub fn get_definition(&self) -> PhpMixed {
        todo!()
    }

    pub fn get_long_version(&self) -> String {
        todo!()
    }

    pub fn find(&self, name: &str) -> anyhow::Result<PhpMixed> {
        todo!()
    }

    pub fn all(&self, namespace: Option<&str>) -> Vec<PhpMixed> {
        todo!()
    }

    pub fn get_namespaces(&self) -> Vec<String> {
        todo!()
    }

    pub fn set_default_command(
        &mut self,
        command_name: &str,
        is_single_command: bool,
    ) -> &mut Self {
        todo!()
    }

    pub fn is_single_command(&self) -> bool {
        todo!()
    }
}
