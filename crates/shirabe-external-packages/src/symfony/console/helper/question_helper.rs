use crate::symfony::console::helper::HelperInterface;
use crate::symfony::console::input::InputInterface;
use crate::symfony::console::output::OutputInterface;
use crate::symfony::console::question::Question;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct QuestionHelper;

impl QuestionHelper {
    pub fn ask(
        &self,
        _input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        _question: &Question,
    ) -> PhpMixed {
        todo!()
    }
}

impl HelperInterface for QuestionHelper {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
