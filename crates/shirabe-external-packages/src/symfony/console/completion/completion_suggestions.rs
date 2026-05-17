use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct CompletionSuggestions;

impl CompletionSuggestions {
    pub fn suggest_values(&mut self, _values: Vec<PhpMixed>) {
        todo!()
    }

    pub fn suggest_value(&mut self, _value: PhpMixed) {
        todo!()
    }
}
