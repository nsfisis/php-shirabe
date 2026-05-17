use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct QuestionHelper;

impl QuestionHelper {
    pub fn ask(
        &self,
        _input: &mut dyn std::any::Any,
        _output: &mut dyn std::any::Any,
        _question: &dyn std::any::Any,
    ) -> Option<PhpMixed> {
        todo!()
    }
}
