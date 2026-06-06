use crate::symfony::component::console::input::InputInterface;
use crate::symfony::component::console::output::OutputInterface;
use crate::symfony::component::console::question::Question;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct QuestionHelper;

impl QuestionHelper {
    pub fn ask(
        &self,
        _input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        _question: &Question,
    ) -> Option<PhpMixed> {
        todo!()
    }
}
