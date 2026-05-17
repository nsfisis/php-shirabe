use crate::symfony::component::console::input::input_interface::InputInterface;
use crate::symfony::component::console::output::output_interface::OutputInterface;
use crate::symfony::component::console::question::question::Question;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct QuestionHelper;

impl QuestionHelper {
    pub fn ask(
        &self,
        _input: &mut dyn InputInterface,
        _output: &mut dyn OutputInterface,
        _question: &Question,
    ) -> Option<PhpMixed> {
        todo!()
    }
}
