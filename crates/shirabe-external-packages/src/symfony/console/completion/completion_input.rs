use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct CompletionInput;

impl CompletionInput {
    pub fn get_completion_type(&self) -> String {
        todo!()
    }

    pub fn get_completion_name(&self) -> Option<String> {
        todo!()
    }

    pub fn get_completion_value(&self) -> String {
        todo!()
    }

    pub fn must_suggest_option_values_for(&self, name: &str) -> bool {
        todo!()
    }

    pub fn must_suggest_argument_values_for(&self, name: &str) -> bool {
        todo!()
    }
}
