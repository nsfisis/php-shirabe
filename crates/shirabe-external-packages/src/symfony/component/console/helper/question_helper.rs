use crate::symfony::component::console::input::input_interface::InputInterface;
use crate::symfony::component::console::output::output_interface::OutputInterface;
use crate::symfony::component::console::question::question::Question;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct QuestionHelper;

impl QuestionHelper {
    pub fn ask(
        &self,
        input: &mut dyn InputInterface,
        output: &mut dyn OutputInterface,
        question: &Question,
    ) -> Option<PhpMixed> {
        todo!()
    }
}
