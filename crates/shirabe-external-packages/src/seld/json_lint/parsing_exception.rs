#[derive(Debug)]
pub struct ParsingException {
    pub message: String,
    pub code: i64,
}

impl ParsingException {
    pub fn new(message: String, _details: Option<shirabe_php_shim::PhpMixed>) -> Self {
        Self { message, code: 0 }
    }

    pub fn get_message(&self) -> &str {
        &self.message
    }

    pub fn get_details(&self) -> indexmap::IndexMap<String, shirabe_php_shim::PhpMixed> {
        // TODO(phase-b): PHP ParsingException exposes ['text', 'line', 'token'] details
        indexmap::IndexMap::new()
    }
}

impl std::fmt::Display for ParsingException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParsingException {}
