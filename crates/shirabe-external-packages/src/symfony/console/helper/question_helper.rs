use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct QuestionHelper;

impl QuestionHelper {
    pub fn ask(
        &self,
        input: &mut dyn std::any::Any,
        output: &mut dyn std::any::Any,
        question: &dyn std::any::Any,
    ) -> Option<PhpMixed> {
        todo!()
    }
}
